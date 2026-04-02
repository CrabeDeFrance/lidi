from contextlib import contextmanager
import os

def build_lidi_config(context, udp_port, log_config):
    mtu = 1500
    if not context.repair_block:
        if context.mtu:
            mtu = context.mtu
            repair_block = 2 * context.mtu
        else:
            repair_block = 3000
    else:
        repair_block = context.repair_block

    if context.block_size:
        block_size = context.block_size
    else:
        block_size = 30000
    
    return f"""
encoding_block_size = {block_size}
repair_block_size = {repair_block}

# IP address and port used to send UDP packets between diode-send and diode-receive
udp_addr = "127.0.0.1"

udp_port = [ {udp_port} ]

# MTU of the to use one the UDP link
udp_mtu = {mtu}

# heartbeat period in ms
heartbeat = 500

# Path to log configuration file
log_config = "{log_config}"

# specific options for diode-send
[sender]
# TCP server socket to accept data
bind_tcp = "127.0.0.1:{context.tcp_send_port}"

# UDP source address to use
bind_udp = "127.0.0.1:0"

# specific options for diode-receive
[receiver]
to_tcp = "127.0.0.1:{context.tcp_receive_port}"
# block_expiration_timeout = 500
session_expiration_timeout = 1000
"""

def write_lidi_config(context, filename, udp_port, log_config):
    """Write LIDI configuration to file."""
    full_path = os.path.join(context.base_dir, filename)
    with open(full_path, "w") as config_file:
        config_file.write(build_lidi_config(context, udp_port, log_config))
    return full_path

@contextmanager
def log_files(base_dir, name):
    """Context manager for handling log files."""
    log_file_path = os.path.join(base_dir, f'{name}.log')
    log_file_error_path = os.path.join(base_dir, f'{name}-error.log')
    
    with open(log_file_path, 'w') as stdout, open(log_file_error_path, 'w') as stderr:
        yield stdout, stderr

def build_lidi_send_command(context):
    lidi_config = write_lidi_config(context, "lidi_send.toml", "5000", context.log_config_diode_send)

    diode_send_command = [f'{context.bin_dir}/diode-send', '-c', lidi_config]

    return diode_send_command

def build_lidi_receive_command(context):
    # Determine UDP port based on network behavior
    has_network_simulator = (
        context.network_down_after or
        context.network_up_after or
        context.network_drop or
        context.network_max_bandwidth or
        context.bandwidth_must_not_exceed
    )
    receiver_bind_udp_port = "6000" if has_network_simulator else "5000"

    lidi_config = write_lidi_config(context, "lidi_receive.toml", receiver_bind_udp_port, context.log_config_diode_receive)

    diode_receive_command = [f'{context.bin_dir}/diode-receive', '-c', lidi_config]

    return diode_receive_command

def build_lidi_receive_file_command(context):
    diode_receive_file_command = [
        f'{context.bin_dir}/diode-receive-file',
        '--bind-tcp',
        f'127.0.0.1:{context.tcp_receive_port}',
        context.receive_dir
    ]

    return diode_receive_file_command

def build_diode_send_dir_command(context):
    diode_send_dir_command = [
        f'{context.bin_dir}/diode-send-dir',
        '--log-config', context.log_config_diode_send_dir,
        '--maximum-files', '1',
        '--to-tcp', f'127.0.0.1:{context.tcp_send_port}',
        context.send_dir
    ]

    return diode_send_dir_command

def build_diode_send_file_command(context, filename):
    # Création de la liste de base pour la commande
    base_command = [
        f"{context.bin_dir}/diode-send-file",
        "--buffer-size",
        "8192",
        "--to-tcp",
        f"127.0.0.1:{context.tcp_send_port}"
    ]
    
    # Convertir filename en liste pour la fusion
    # Si filename est déjà une liste, l'utiliser telle quelle
    # Sinon, le mettre dans une liste
    if isinstance(filename, list):
        filename_list = filename
    else:
        filename_list = [filename]
    
    # Fusion des deux listes : la commande de base et la liste contenant filename
    diode_send_file_command = base_command + filename_list
    
    return diode_send_file_command

def build_network_simulator_command(context):
    # Setup network behavior parameters
    network_simulator_command = [
        f'{context.bin_dir}/network-behavior',
        '--bind-udp', '0.0.0.0:5000',
        '--to-udp', '127.0.0.1:6000',
        '--log-config', context.log_config_network_behavior
    ]
    
    # Add network behavior options
    network_options = [
        ('network_down_after', '--network-down-after'),
        ('network_up_after', '--network-up-after'),
        ('network_drop', '--loss-rate'),
        ('network_max_bandwidth', '--max-bandwidth'),
        ('bandwidth_must_not_exceed', '--abort-on-max-bandwidth')
    ]
    
    use_network_simulator = False
    for attr_name, option in network_options:
        attr_value = getattr(context, attr_name, None)
        if attr_value:
            network_simulator_command.extend([option, str(attr_value)])
            use_network_simulator = True

    if not use_network_simulator:
        return None
    else:
        return network_simulator_command