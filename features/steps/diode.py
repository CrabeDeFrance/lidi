# implementation of steps for "diode start"

from behave import given, when, then, use_step_matcher
import subprocess
import time
import psutil
import os
from tempfile import TemporaryDirectory

from throttle_fs import ThrottledFSProcess

use_step_matcher("cfparse")

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

    if context.read_rate:
        max_bandwidth = "max_bandwidth = {}".format(context.read_rate)
    else:
        max_bandwidth = ""
  
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
{log_config}

# specific options for diode-send
[sender]
# TCP server socket to accept data
bind_tcp = "127.0.0.1:5000"

# UDP source address to use
bind_udp = "127.0.0.1:0"

# ratelimit TCP session speed (in Mbit/s)
{max_bandwidth}

# specific options for diode-receive
[receiver]
to_tcp = "127.0.0.1:7000"
# block_expiration_timeout = 500
session_expiration_timeout = 1000
"""

def write_lidi_config(context, filename, udp_port, log_config):
    filename = os.path.join("/dev/shm", filename)
    log_config_str = f"log_config = \"{log_config}\""
    with open(filename, "w") as f:
        f.write(build_lidi_config(context, udp_port, log_config_str))
        f.close()
    return filename


def nice(process_name):
    for proc in psutil.process_iter():
        if process_name in proc.name():
            ps = psutil.Process(proc.pid)
            # must be root
            if os.getuid() == 0:
                ps.nice(-20)
            return 

def start_diode_receive(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    if context.network_down_after or context.network_up_after or context.network_drop:
        receiver_bind_udp_port = "6000"
    else:
        receiver_bind_udp_port = "5000"

    lidi_config = write_lidi_config(context, "lidi_receive.toml", receiver_bind_udp_port, context.log_config_diode_receive)

    diode_receive_command = [f'{context.bin_dir}/diode-receive', '-c', lidi_config]

    context.proc_diode_receive = subprocess.Popen(diode_receive_command, stdout=stdout, stderr=stderr)
    # here we need to wait enough time for diode-receive to be ready
    time.sleep(2)
    poll = context.proc_diode_receive.poll()
    if poll:
        print(context.proc_diode_receive.communicate())
        raise Exception("Can't start diode receive")

    nice('diode-receive')

def stop_diode_receive(context):
    if context.proc_diode_receive:
        context.proc_diode_receive.kill()

def start_diode_send(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    lidi_config = write_lidi_config(context, "lidi_send.toml", "5000", context.log_config_diode_send)

    diode_send_command = [f'{context.bin_dir}/diode-send', '-c', lidi_config]

    context.proc_diode_send = subprocess.Popen(diode_send_command, stdout=stdout, stderr=stderr)
    time.sleep(0.5)
    poll = context.proc_diode_send.poll()
    if poll:
        print(context.proc_diode_send.communicate())
        raise Exception("Can't start diode send")
    nice('diode-send')

def stop_diode_send(context):
    if context.proc_diode_send:
        context.proc_diode_send.kill()

def start_diode(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    network_behavior = False
    network_command = [f'{context.bin_dir}/network-behavior', '--bind-udp', '0.0.0.0:5000', '--to-udp', '127.0.0.1:6000',
                       '--log-config', context.log_config_network_behavior]
    if context.network_down_after:
        network_command.append('--network-down-after')
        network_command.append(str(context.network_down_after))
        network_behavior = True

    if context.network_up_after:
        network_command.append('--network-up-after')
        network_command.append(str(context.network_up_after))
        network_behavior = True

    if context.network_drop:
        network_command.append('--loss-rate')
        network_command.append(context.network_drop)
        network_behavior = True

    if network_behavior:
        context.proc_network = subprocess.Popen(network_command)
        time.sleep(1)

    # start diode-receive-file (tcp server)
    diode_receive_file_command = [f'{context.bin_dir}/diode-receive-file', '--bind-tcp', '127.0.0.1:7000', context.receive_dir.name]
    if context.log_config_diode_receive_file:
        diode_receive_file_command.append('--log-config')
        diode_receive_file_command.append(context.log_config_diode_receive_file)

    context.proc_diode_receive_file = subprocess.Popen(
        diode_receive_file_command,
        stdout=stdout, stderr=stderr)

    time.sleep(1)

    # start diode-receive (connects to diode-receive-file)
    start_diode_receive(context)

    # finally start diode-send (send init packet to diode-receive, acts as a server for diode-send-file)
    start_diode_send(context)


def start_throttled_diode(context, read_rate):
    context.send_ratelimit_dir = TemporaryDirectory()

    context.proc_throttled_fs = ThrottledFSProcess(context.send_ratelimit_dir.name, context.send_dir.name, read_rate)
    context.proc_throttled_fs.start()

    time.sleep(1)

    start_diode(context)

def start_diode_send_dir(context):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    diode_send_dir_command = [f'{context.bin_dir}/diode-send-dir', '--log-config', context.log_config_diode_send_dir, '--maximum-delay', '200', '--to-tcp', '127.0.0.1:5000', context.send_dir.name]

    context.proc_diode_send_dir = subprocess.Popen(
        diode_send_dir_command,
        stdout=stdout, stderr=stderr)

    time.sleep(1)

@given('diode is started')
def step_impl(context):
    start_diode(context)

@when('diode-receive is restarted')
def step_impl(context):
    stop_diode_receive(context)
    # wait some time to prevent address already in use if restarted too quickly
    time.sleep(5)
    start_diode_receive(context)

@when('diode-send is restarted')
def step_impl(context):
    stop_diode_send(context)
    start_diode_send(context)

@when('diode-send-dir is started')
def step_impl(context):
    start_diode_send_dir(context)

@given('diode is started with max throughput of {throughput} Mb/s')
def step_diode_started_with_max_throughput(context, throughput):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput)
    start_throttled_diode(context, int(context.read_rate * 1000000 / 8))

@given('diode is started with max throughput of {throughput} Mb/s and MTU {mtu}')
def step_diode_started_with_max_throughput(context, throughput, mtu):
    # two possibilities : limit file system read throughput or configure the diode for that
    context.read_rate = int(throughput)
    context.mtu = int(mtu)
    start_throttled_diode(context, int(context.read_rate * 1000000 / 8))

@given('encoding block size is {encoding} and repair block size is {repair}')
def step_set_encoding_repair_block_size(context, encoding, repair):
    context.repair_block = 20000
    context.block_size = 20000
