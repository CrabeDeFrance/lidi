use std::fs::File;
use std::io::{self, Read};
use std::str::FromStr;

// Struct to represent the parsed data
#[derive(Debug)]
struct ParsedEntry {
    _sl: u32,
    local_address: String,
    _rem_address: String,
    st: u8,
    tx_rx_queue: String,
    _tr_tm_when: String,
    retrnsmt: u32,
    uid: u32,
    timeout: u32,
    _inode: u32,
    _ref_count: u32,
    _pointer: String,
    drops: u64,
}

// Function to parse a single line of input
fn parse_line(line: &str) -> Result<ParsedEntry, &'static str> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() != 13 {
        return Err("Input line does not have the expected number of fields");
    }

    Ok(ParsedEntry {
        _sl: u32::from_str(parts[0].trim_end_matches(':')).unwrap_or(0),
        local_address: parts[1].to_string(),
        _rem_address: parts[2].to_string(),
        st: u8::from_str(parts[3]).unwrap_or(0),
        tx_rx_queue: parts[4].to_string(),
        _tr_tm_when: parts[5].to_string(),
        retrnsmt: u32::from_str(parts[6]).unwrap_or(0),
        uid: u32::from_str(parts[7]).unwrap_or(0),
        timeout: u32::from_str(parts[8]).unwrap_or(0),
        _inode: u32::from_str(parts[9]).unwrap_or(0),
        _ref_count: u32::from_str(parts[10]).unwrap_or(0),
        _pointer: parts[11].to_string(),
        drops: u64::from_str(parts[12]).unwrap_or(0),
    })
}

fn clear_screen() {
    // ANSI escape code to clear the screen
    print!("\x1B[2J\x1B[H");
}

fn read_udp(max_rx: &mut [u32; 10]) -> io::Result<()> {
    let ports = [5000, 5001, 5002, 5003, 5004, 5005, 5006, 5007, 5008, 5009];
    // Specify the file path
    let file_path = "/proc/net/udp";

    // Open the file
    let mut file = File::open(file_path)?;

    // Create a string to hold the file content
    let mut content = String::new();

    // Read the file content into the string
    file.read_to_string(&mut content)?;

    // Split the string into lines and skip the first line
    let lines = content.lines().skip(1);

    lines.for_each(|line| {
        // Print the file content
        match parse_line(line) {
            Ok(parsed_entry) => {
                //   Split the string by the colon and take the second part
                let parts: Vec<&str> = parsed_entry.local_address.split(':').collect();
                if let Some(hex_part) = parts.get(1) {
                    // Parse the hex part into a decimal u32
                    match u32::from_str_radix(hex_part, 16) {
                        Ok(decimal_value) => {
                            if ports.contains(&decimal_value) {

                                let parts: Vec<&str> = parsed_entry.tx_rx_queue.split(':').collect();
                                if let Some(hex_part) = parts.get(1) {
                                    // Parse the hex part into a decimal u32
                                    match u32::from_str_radix(hex_part, 16) {
                                        Ok(rx_size) => {

                                            let idx = decimal_value as usize - 5000;
                                            if max_rx[idx] < rx_size {
                                                max_rx[idx] = rx_size;
                                            }

                                            println!(
                                                "port: {} st: {} rx_queue: {:07}/{:07} retrnsmt: {} uid: {} timeout: {} drops: {}",
                                                decimal_value, parsed_entry.st, rx_size, max_rx[idx], parsed_entry.retrnsmt, parsed_entry.uid, parsed_entry.timeout, parsed_entry.drops
                                            );
                                        }
                                        Err(e) => {
                                            eprintln!("Error parsing hex: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error parsing hex: {}", e);
                        }
                    }
                } else {
                    eprintln!("Invalid input format: No colon found.");
                }
            }
            Err(e) => {
                eprintln!("Error parsing input: {}", e);
            }
        }
    });

    Ok(())
}

// Define a struct to hold the parsed values
#[derive(Debug)]
struct IpStats {
    pub _forwarding: u64,
    pub _default_ttl: u64,
    pub _in_receives: u64,
    pub in_hdr_errors: u64,
    pub _in_addr_errors: u64,
    pub _forw_datagrams: u64,
    pub _in_unknown_protos: u64,
    pub in_discards: u64,
    pub _in_delivers: u64,
    pub _out_requests: u64,
    pub out_discards: u64,
    pub _out_no_routes: u64,
    pub _reasm_timeout: u64,
    pub _reasm_reqds: u64,
    pub _reasm_oks: u64,
    pub _reasm_fails: u64,
    pub _frag_oks: u64,
    pub _frag_fails: u64,
    pub _frag_creates: u64,
}

fn read_snmp() -> io::Result<()> {
    // Specify the file path
    let file_path = "/proc/net/snmp";

    // Open the file
    let mut file = File::open(file_path)?;

    // Create a string to hold the file content
    let mut content = String::new();

    // Read the file content into the string
    file.read_to_string(&mut content)?;

    let mut ip_count = 0;
    let mut udp_count = 0;

    content.lines().for_each(|line| {
        if line.starts_with("Ip:") {
            ip_count += 1;
            if ip_count == 2 {
                if let Some(ip) = parse_ip_stats(line) {
                    println!(
                        "in_discards: {} out_discards: {}, in_hdr_errors: {}",
                        ip.in_discards, ip.out_discards, ip.in_hdr_errors
                    );
                } else {
                    eprintln!("error parsing ip");
                }
            }
        }

        if line.starts_with("Udp:") {
            udp_count += 1;
            if udp_count == 2 {
                if let Some(udp) = parse_udp_stats(line) {
                    println!(
                        "in_error: {} rcvbuf_errors: {}, sndbuf_errors: {} in_csum_errors: {} mem_errors: {}, no_ports: {}",
                        udp.in_errors, udp.rcvbuf_errors, udp.sndbuf_errors, udp.in_csum_errors, udp.mem_errors, udp.no_ports
                    );
                } else {
                    eprintln!("error parsing udp");
                }
            }
        }
    });

    Ok(())
}

// Function to parse the header and values into the IpStats struct
fn parse_ip_stats(buffer: &str) -> Option<IpStats> {
    // Split the second line by whitespace to extract the numeric values
    let values: Vec<&str> = buffer.split_whitespace().collect();

    // Ensure the right number of values (19 expected, after "Ip:")
    if values.len() != 20 {
        return None; // Unexpected number of values
    }

    // Parse the values and construct the IpStats struct
    Some(IpStats {
        _forwarding: u64::from_str(values[1]).ok()?,
        _default_ttl: u64::from_str(values[2]).ok()?,
        _in_receives: u64::from_str(values[3]).ok()?,
        in_hdr_errors: u64::from_str(values[4]).ok()?,
        _in_addr_errors: u64::from_str(values[5]).ok()?,
        _forw_datagrams: u64::from_str(values[6]).ok()?,
        _in_unknown_protos: u64::from_str(values[7]).ok()?,
        in_discards: u64::from_str(values[8]).ok()?,
        _in_delivers: u64::from_str(values[9]).ok()?,
        _out_requests: u64::from_str(values[10]).ok()?,
        out_discards: u64::from_str(values[11]).ok()?,
        _out_no_routes: u64::from_str(values[12]).ok()?,
        _reasm_timeout: u64::from_str(values[13]).ok()?,
        _reasm_reqds: u64::from_str(values[14]).ok()?,
        _reasm_oks: u64::from_str(values[15]).ok()?,
        _reasm_fails: u64::from_str(values[16]).ok()?,
        _frag_oks: u64::from_str(values[17]).ok()?,
        _frag_fails: u64::from_str(values[18]).ok()?,
        _frag_creates: u64::from_str(values[19]).ok()?,
    })
}

// Define a struct to hold the parsed values
#[derive(Debug)]
struct UdpStats {
    _in_datagrams: u64,
    no_ports: u64,
    in_errors: u64,
    _out_datagrams: u64,
    rcvbuf_errors: u64,
    sndbuf_errors: u64,
    in_csum_errors: u64,
    _ignored_multi: u64,
    mem_errors: u64,
}

// Function to parse the header and values into the UdpStats struct
fn parse_udp_stats(buffer: &str) -> Option<UdpStats> {
    // Split the second line by whitespace to extract the numeric values
    let values: Vec<&str> = buffer.split_whitespace().collect();

    // Ensure the right number of values (9 expected, after "Udp:")
    if values.len() != 10 {
        return None; // Unexpected number of values
    }

    // Parse the values and construct the UdpStats struct
    Some(UdpStats {
        _in_datagrams: u64::from_str(values[1]).ok()?,
        no_ports: u64::from_str(values[2]).ok()?,
        in_errors: u64::from_str(values[3]).ok()?,
        _out_datagrams: u64::from_str(values[4]).ok()?,
        rcvbuf_errors: u64::from_str(values[5]).ok()?,
        sndbuf_errors: u64::from_str(values[6]).ok()?,
        in_csum_errors: u64::from_str(values[7]).ok()?,
        _ignored_multi: u64::from_str(values[8]).ok()?,
        mem_errors: u64::from_str(values[9]).ok()?,
    })
}

fn main() -> io::Result<()> {
    let mut max_rx = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    loop {
        if let Err(_e) = read_udp(&mut max_rx) {
            return Ok(());
        }

        if let Err(_e) = read_snmp() {
            return Ok(());
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
        clear_screen();
    }
}
