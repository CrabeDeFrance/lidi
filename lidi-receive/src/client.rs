//! Worker that writes decoded and reordered messages to client

use crate::ClientLifecycle;
use lidi_command_utils::config;
use lidi_protocol as protocol;
use std::io::Write;

pub fn start<Lifecycle>(
    receiver: &crate::Receiver<Lifecycle>,
    endpoint_id: protocol::EndpointId,
    endpoint: &config::Endpoint,
    client_id: protocol::ClientId,
    for_client: &crossbeam_channel::Receiver<protocol::Block>,
) -> Result<(), crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    let endpoint_options = endpoint.options();

    log::info!(
        "client {client_id:x}: starting transfer to endpoint {endpoint_id} ({endpoint_options})"
    );

    let mut client = receiver.client_lifecycle.start(endpoint, client_id)?;

    // Gives the block's buffer back to reblock instead of dropping it, mirroring the
    // udp<->reblock packet-batch recycler. Ignores the error: if every reblock thread is gone
    // there's nothing to recycle into, or the (bounded) pool is already full, the `Vec` is
    // simply dropped.
    //
    // In practice only ever called for Data/End/Abort: dispatch never forwards Start or
    // Heartbeat blocks to a client's queue (see dispatch.rs), so `for_client.recv()` never
    // yields one, but every arm calls `recycle` regardless so this stays true if that changes.
    let recycle = |receiver: &crate::Receiver<Lifecycle>, block: protocol::Block| {
        let _ = receiver.decode_buf_recycler_tx.try_send(block.into_data());
    };

    loop {
        let block = for_client.recv()?;

        let payload = block.payload();

        match block.block_type()? {
            protocol::BlockType::Data => {
                client.write_all(payload)?;
                if endpoint_options.flush {
                    client.flush()?;
                }
                recycle(receiver, block);
            }
            protocol::BlockType::End => {
                client.write_all(payload)?;
                client.flush()?;
                if let Err(e) = receiver.client_lifecycle.end(client, true) {
                    log::error!("client {client_id:x}: {e}");
                }
                recycle(receiver, block);
                break;
            }
            protocol::BlockType::Abort => {
                if let Err(e) = receiver.client_lifecycle.end(client, false) {
                    log::error!("client {client_id:x}: {e}");
                }
                recycle(receiver, block);
                break;
            }
            protocol::BlockType::Start | protocol::BlockType::Heartbeat => {
                recycle(receiver, block);
            }
        }
    }

    Ok(())
}
