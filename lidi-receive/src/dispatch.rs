//! Worker that manages active transfers queue and dispatch incoming [`crate::protocol`]
//! blocks to clients

use crate::ClientLifecycle;
use lidi_protocol as protocol;
#[cfg(feature = "heartbeat")]
use std::time;
use std::{collections, thread};

const WINDOW_WIDTH: protocol::ClientId = protocol::ClientId::MAX / 2;

pub enum Message {
    NewSession(protocol::SessionId),
    LostBlock,
    Block(protocol::SessionId, protocol::Block),
}

#[allow(clippy::too_many_lines)]
pub fn start<Lifecycle>(receiver: &crate::Receiver<Lifecycle>) -> Result<(), crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    #[cfg(feature = "heartbeat")]
    let mut last_heartbeat = time::Instant::now();
    #[cfg(feature = "heartbeat")]
    let heartbeat_check = receiver.config.heartbeat.map(|hb| {
        // Add 25% time between each heartbeat checks to let some time
        // for the heartbeat block to arrive
        hb.mul_f32(1.25)
    });

    let mut session_id = 0;

    let mut pending_start = collections::HashMap::new();

    loop {
        #[cfg(not(feature = "heartbeat"))]
        let message = receiver.for_dispatch.recv()?;
        #[cfg(feature = "heartbeat")]
        let message = match heartbeat_check.as_ref() {
            None => receiver.for_dispatch.recv()?,
            Some(hb_check) => match receiver.for_dispatch.recv_timeout(*hb_check) {
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    let hb = receiver.config.heartbeat.as_ref().unwrap();
                    if last_heartbeat.elapsed() > *hb {
                        #[cfg(feature = "prometheus")]
                        metrics::counter!("lidi_receive_heartbeat_missed").increment(1);
                        log::warn!("no heartbeat block received for {} second(s)", hb.as_secs());
                    }
                    continue;
                }
                message => message?,
            },
        };

        let block = match message {
            Message::LostBlock => {
                pending_start.clear();

                let actives = receiver.active_transfers.clone();
                for (client_id, client_sendq) in actives {
                    receiver.failed_transfers.insert(client_id);

                    let block = protocol::Block::new(
                        None,
                        protocol::BlockType::Abort,
                        &receiver.raptorq,
                        client_id,
                        0,
                        None,
                    )?;

                    if let Err(e) = client_sendq.try_send(block) {
                        #[cfg(feature = "prometheus")]
                        metrics::counter!("lidi_receive_client_queue_full").increment(1);
                        log::error!("failed to send payload to client {client_id:x}: {e}");
                    }
                }
                receiver.active_transfers.clear();

                continue;
            }
            Message::NewSession(new_session_id) => {
                if new_session_id != session_id {
                    session_id = new_session_id;

                    log::info!("new session is {new_session_id:x}");

                    pending_start.clear();

                    let actives = receiver.active_transfers.clone();
                    for (client_id, client_sendq) in actives {
                        let block = protocol::Block::new(
                            None,
                            protocol::BlockType::Abort,
                            &receiver.raptorq,
                            client_id,
                            0,
                            None,
                        )?;

                        if let Err(e) = client_sendq.try_send(block) {
                            #[cfg(feature = "prometheus")]
                            metrics::counter!("lidi_receive_client_queue_full").increment(1);
                            log::error!("failed to send payload to client {client_id:x}: {e}");
                        }
                    }
                    receiver.active_transfers.clear();

                    receiver.failed_transfers.clear();
                }

                continue;
            }
            Message::Block(block_session_id, block) => {
                if block_session_id == session_id {
                    block
                } else {
                    log::debug!("ignoring block from old session");
                    continue;
                }
            }
        };

        log::trace!("received {block}");

        let block_type = match block.block_type() {
            Err(e) => {
                log::error!("block of UNKNOWN type received ({e}), dropping it");
                continue;
            }
            Ok(mt) => mt,
        };

        let client_id = block.client_id();

        if receiver.failed_transfers.contains(&client_id) {
            log::trace!("ignoring packet for failed client {client_id:x}");
            continue;
        }

        match block_type {
            protocol::BlockType::Heartbeat => {
                #[cfg(feature = "heartbeat")]
                {
                    log::debug!("heartbeat received");
                    last_heartbeat = time::Instant::now();
                }

                // Never forwarded to a client, so recycle it back here instead of dropping
                // it, mirroring lidi-send's block_recycler and receive's packet_vec_recycler.
                let _ = receiver.decode_buf_recycler_tx.try_send(block.into_data());
            }

            protocol::BlockType::Start => {
                let payload = block.payload();

                match protocol::EndpointId::deserialize(payload) {
                    None => {
                        log::error!("client {client_id:x} for invalid endpoint");
                        receiver.failed_transfers.insert(client_id);
                    }
                    Some(endpoint_id) => {
                        let (client_sendq, client_recvq) =
                            pending_start.remove(&client_id).unwrap_or_else(|| {
                                if 0 < receiver.config.client_queue_size {
                                    crossbeam_channel::bounded(receiver.config.client_queue_size)
                                } else {
                                    crossbeam_channel::unbounded()
                                }
                            });

                        #[cfg(feature = "prometheus")]
                        #[allow(clippy::cast_precision_loss)]
                        metrics::gauge!("lidi_receive_pending_start_count")
                            .set(pending_start.len() as f64);

                        receiver
                            .active_transfers
                            .entry(client_id)
                            .insert(client_sendq);

                        receiver
                            .to_clients
                            .send((endpoint_id, client_id, client_recvq))?;

                        let mut id = client_id;
                        let last = id.wrapping_add(WINDOW_WIDTH);
                        while id != last {
                            receiver.failed_transfers.remove(&id);
                            id = id.wrapping_add(1);
                        }
                    }
                }

                // Only the EndpointId was extracted above; the block itself is never
                // forwarded, so recycle it back here instead of dropping it, mirroring
                // lidi-send's block_recycler and receive's packet_vec_recycler.
                let _ = receiver.decode_buf_recycler_tx.try_send(block.into_data());
            }

            protocol::BlockType::Data | protocol::BlockType::Abort | protocol::BlockType::End => {
                match receiver.active_transfers.entry(client_id) {
                    dashmap::Entry::Occupied(oe) => {
                        if let Err(e) = oe.get().try_send(block) {
                            #[cfg(feature = "prometheus")]
                            metrics::counter!("lidi_receive_client_queue_full").increment(1);
                            log::error!("failed to send block to client {client_id:x}: {e}");
                            oe.remove();
                            receiver.failed_transfers.insert(client_id);
                        }
                    }
                    dashmap::Entry::Vacant(_) => {
                        match pending_start.entry(client_id) {
                            collections::hash_map::Entry::Occupied(oe) => {
                                if let Err(e) = oe.get().0.try_send(block) {
                                    #[cfg(feature = "prometheus")]
                                    metrics::counter!("lidi_receive_client_queue_full")
                                        .increment(1);
                                    log::error!(
                                        "failed to send block to client {client_id:x}: {e}"
                                    );
                                    oe.remove();
                                    receiver.failed_transfers.insert(client_id);
                                }
                            }
                            collections::hash_map::Entry::Vacant(ve) => {
                                let (client_sendq, client_recvq) = if 0 < receiver
                                    .config
                                    .client_queue_size
                                {
                                    crossbeam_channel::bounded(receiver.config.client_queue_size)
                                } else {
                                    crossbeam_channel::unbounded()
                                };

                                if client_sendq
                                    .try_send(block)
                                    .inspect_err(|e| {
                                        #[cfg(feature = "prometheus")]
                                        metrics::counter!("lidi_receive_client_queue_full")
                                            .increment(1);
                                        log::error!(
                                            "failed to send block to client {client_id:x}: {e}"
                                        );
                                        receiver.failed_transfers.insert(client_id);
                                    })
                                    .is_ok()
                                {
                                    ve.insert((client_sendq, client_recvq));
                                } else {
                                    receiver.failed_transfers.insert(client_id);
                                }
                            }
                        }

                        #[cfg(feature = "prometheus")]
                        #[allow(clippy::cast_precision_loss)]
                        metrics::gauge!("lidi_receive_pending_start_count")
                            .set(pending_start.len() as f64);
                    }
                }
            }
        }

        thread::yield_now();
    }
}
