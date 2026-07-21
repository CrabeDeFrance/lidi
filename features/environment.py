# functions to be called before or after tests must be put here

import signal
import subprocess
import time
import os
from pathlib import Path
import psutil

from features.steps.lidi import stop_throttled_diode
from features.steps.utils import kill_process_safe
from features.steps.tls_pki import generate_pki

_LIDI_PROCESS_NAMES = {
    'lidi-send', 'lidi-receive', 'lidi-file-send', 'lidi-file-receive',
    'lidi-dir-send', 'lidi-network-simulator',
}

def wait_for_processes_dead(max_wait=1.0):
    """Return as soon as no lidi processes remain, up to max_wait seconds."""
    deadline = time.monotonic() + max_wait
    while time.monotonic() < deadline:
        try:
            if not any(p.info['name'] in _LIDI_PROCESS_NAMES
                       for p in psutil.process_iter(['name'])):
                return True
        except psutil.Error:
            pass
        time.sleep(0.02)
    os.system("pkill -9 -f lidi 2>/dev/null")
    return False


# function called before any feature or scenario
def before_all(context):
    # build all applications before running any test
    proc = subprocess.Popen(['just', 'release'])
    proc.communicate()

    # build lidi-clients without the inotify feature, to test the directory
    # polling fallback used by lidi-dir-send --watch when inotify is not
    # available
    proc = subprocess.Popen([
        'cargo', 'build', '--release',
        '--package', 'lidi-clients',
        '--no-default-features',
        '--features', 'hash,log4rs,tcp,tls,unix',
        '--target-dir', 'target/no-inotify',
    ])
    proc.communicate()

    # build lidi-send/lidi-receive without mimalloc, used by the Valgrind
    # scenarios (see justfile: release_no_mimalloc)
    proc = subprocess.Popen(['just', 'release_no_mimalloc'])
    proc.communicate()


# function called before each test: initialize context with default values
def before_scenario(context, _feature):
    # test temp dir
    context.base_dir="/dev/shm/lidi"
    
    if not os.path.isdir(context.base_dir):
        os.mkdir(context.base_dir)

    # delete all files in folder (keep directories)
    try:
         files = os.listdir(context.base_dir)
         for file in files:
             file_path = os.path.join(context.base_dir, file)
             if os.path.isfile(file_path):
                 os.remove(file_path)
    except OSError:
        print("Error occurred while deleting files.")

    # Use explicit, static paths for directories
    context.send_dir = os.path.join(context.base_dir, "send")
    context.send_ratelimit_dir = None
    context.receive_dir = os.path.join(context.base_dir, "receive")
    context.log_dir = os.path.join(context.base_dir, "log")
    
    # Clean up directories from previous test
    for directory in [context.send_dir, context.receive_dir, context.log_dir]:
        try:
            if os.path.isdir(directory):
                import shutil
                shutil.rmtree(directory)
        except Exception as e:
            print(f"Error cleaning up directory {directory}: {e}")
    
    # Create directories if they don't exist
    os.makedirs(context.send_dir, exist_ok=True)
    os.makedirs(context.receive_dir, exist_ok=True)
    os.makedirs(context.log_dir, exist_ok=True)

    # Generate test PKI for TLS tests
    context.pki_dir = Path(context.base_dir) / 'pki'
    try:
        generate_pki(context.pki_dir)
    except Exception as e:
        print(f"Warning: Failed to generate PKI: {e}")

    # files metadata
    context.files = {}

    # concurrent processes for multi-client tests
    context.concurrent_processes = []

    # process instances
    context.proc_lidi_receive = None
    context.proc_lidi_send = None
    context.proc_lidi_send_file = None
    context.proc_lidi_send_dir = None
    context.proc_network = None
    context.proc_lidi_receive_file = None
    
    # directory containing binaries
    context.bin_dir = "./target/release/"

    # directory containing lidi-clients binaries built without the inotify
    # feature (used to test the directory polling fallback)
    context.bin_dir_no_inotify = "./target/no-inotify/release/"

    # directory containing lidi-send/lidi-receive binaries built without
    # mimalloc (used by the Valgrind scenarios)
    context.bin_dir_no_mimalloc = "./target/no-mimalloc/release/"
    
    # some possible options
    context.network_down_after = None
    context.network_up_after = None
    context.network_max_bandwidth = None
    context.bandwidth_must_not_exceed = None
    context.network_drop = None
    context.read_rate = None

    # port configuration
    context.tcp_send_port = 4000
    context.tcp_receive_port = 6000

    context.block_size = None
    context.repair = None
    context.mtu = None

    # display
    context.log_config_lidi_receive = None
    context.log_config_lidi_receive_file = None
    context.log_config_lidi_send = None
    context.log_config_lidi_send_dir = None
    context.log_config_lidi_send_file = None
    context.log_config_network_behavior = None

    context.lidi_config_path = context.base_dir

    # file_receive scenario state
    context._file_receive_suspended = False
    context._receive_dir_was_readonly = False

    # setup logging configuration
    setup_log_config(context, context.log_dir)

# function called after every test : cleanup (delete temp directories & kill processes)
def after_scenario(context, _scenario):
    # Resume a SIGSTOPped lidi-file-receive so SIGKILL is delivered cleanly.
    # SIGKILL works on stopped processes too, but SIGCONT first avoids any edge cases.
    proc_frc = getattr(context, 'proc_lidi_receive_file', None)
    if proc_frc and proc_frc.poll() is None and context._file_receive_suspended:
        try:
            os.kill(proc_frc.pid, signal.SIGCONT)
        except (ProcessLookupError, OSError):
            pass

    # Restore receive_dir permissions so shutil.rmtree can clean it up.
    if context._receive_dir_was_readonly:
        try:
            os.chmod(context.receive_dir, 0o755)
        except OSError:
            pass

    stop_throttled_diode(context)

    # Kill concurrent processes from multi-client tests
    if hasattr(context, 'concurrent_processes'):
        for proc_info in context.concurrent_processes:
            proc = proc_info.get('process')
            if proc and proc.poll() is None:
                try:
                    proc.terminate()
                    proc.wait(timeout=5)
                except (subprocess.TimeoutExpired, ProcessLookupError):
                    try:
                        proc.kill()
                        proc.wait(timeout=2)
                    except (subprocess.TimeoutExpired, ProcessLookupError):
                        pass
        context.concurrent_processes.clear()

    # first kill processes
    kill_process_safe('proc_lidi_receive', 'lidi-receive', context)
    kill_process_safe('proc_lidi_send', 'lidi-send', context)
    kill_process_safe('proc_lidi_send_file', 'lidi-file-send', context)
    kill_process_safe('proc_lidi_send_dir', 'lidi-dir-send', context)
    kill_process_safe('proc_network', 'lidi-network-simulator', context)
    kill_process_safe('proc_lidi_receive_file', 'lidi-file-receive', context)

    # Wait until all lidi processes are dead before starting the next scenario
    wait_for_processes_dead()

    # Clear files metadata
    context.files.clear()

def build_log_config(filename, level):
    return f"""
appenders:
  file:
    kind: file
    path: {filename}

root:
  level: {level}
  appenders:
    - file
"""

def setup_log_config(context, log_dir, level="info"):
    context.log_config_lidi_receive = os.path.join(log_dir, "log_config_lidi_receive.yml")
    filename = os.path.join(log_dir, "lidi_receive.log")
    with open(context.log_config_lidi_receive, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_receive_file = os.path.join(log_dir, "log_config_lidi_receive_file.yml")
    filename = os.path.join(log_dir, "lidi_receive_file.log")
    with open(context.log_config_lidi_receive_file, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send = os.path.join(log_dir, "log_config_lidi_send.yml")
    filename = os.path.join(log_dir, "lidi_send.log")
    with open(context.log_config_lidi_send, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send_dir = os.path.join(log_dir, "log_config_lidi_send_dir.yml")
    filename = os.path.join(log_dir, "lidi_send_dir.log")
    with open(context.log_config_lidi_send_dir, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_lidi_send_file= os.path.join(log_dir, "log_config_lidi_send_file.yml")
    filename = os.path.join(log_dir, "lidi_send_file.log")
    with open(context.log_config_lidi_send_file, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

    context.log_config_network_behavior = os.path.join(log_dir, "log_config_network_behavior.yml")
    filename = os.path.join(log_dir, "network_behavior.log")
    with open(context.log_config_network_behavior, "w") as f:
        f.write(build_log_config(filename, level))
        f.close()

