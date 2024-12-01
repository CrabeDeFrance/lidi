# implementation of steps for diode-file-send and diode-file-receive

from behave import when, then, use_step_matcher
import subprocess
import time
import os
import hashlib
import shutil
from tempfile import TemporaryDirectory
from diode import stop_diode_send, start_diode, start_diode_send, start_diode_send_dir, stop_diode_receive, start_diode_receive

use_step_matcher("cfparse")

def md5sum(filename, blocksize=65536):
    h = hashlib.md5()
    with open(filename, "rb") as f:
        for block in iter(lambda: f.read(blocksize), b""):
            h.update(block)
    return h.hexdigest()

def create_file(context, filename, count, blocksize):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT
    # create file
    proc = subprocess.run(
        f'dd if=/dev/random of={filename} bs={blocksize} count={count}',
        stdout=stdout,
        stderr=stderr,
        shell=True,
        timeout=30
    )
    assert proc.returncode == 0

    # store info in context
    store_file_info(context, filename)

def store_file_info(context, filename):

    # store info about the generated file in context
    size = os.stat(filename).st_size
    h = md5sum(filename)

    name = os.path.basename(filename)

    context.files[name] = { 'size': size, 'hash': h, 'path': filename }

def send_file(context, name, count, blocksize, background=False):
    if context.quiet:
        stdout = subprocess.DEVNULL
        stderr = subprocess.DEVNULL
    else:
        stdout = subprocess.PIPE
        stderr = subprocess.STDOUT

    filename = os.path.join(context.send_dir.name, name)
    create_file(context, filename, count, blocksize)

    # take care of possible throttled fs to limit tx throughput
    if context.send_ratelimit_dir:
        filename = os.path.join(context.send_ratelimit_dir.name, name)

    # send it (using buffer size of 8192 to limit bursts & packet drops)
    if not background:
        result = subprocess.run(
#            f'cargo run --release --bin diode-send-file -- --buffer-size 8192 --to-tcp 127.0.0.1:5000 {filename}',
            f'{context.bin_dir}/diode-send-file --buffer-size 8192 --to-tcp 127.0.0.1:5000 {filename}',
            stdout=stdout,
            stderr=stderr,
            shell=True,
            timeout=60
        )

        assert result.returncode == 0
    else:
        subprocess.Popen([f"{context.bin_dir}/diode-send-file", "--buffer-size", "8192", "--to-tcp", "127.0.0.1:5000", filename])

def send_multiple_files(context):
    files = "" 

    for file in context.files:
        files += file + " "

    result = subprocess.run(
        f'{context.bin_dir}/diode-send-file --buffer-size 8192 --to-tcp 127.0.0.1:5000 {files}',
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        shell=True,
        timeout=60
    )

    assert result.returncode == 0


def test_file(context, dir, name, seconds):
    # get info about the file
    info = context.files[name]
    size = info['size']
    h = info['hash']

    # where it should be
    filename = os.path.join(dir, name)

    # wait for it
    seconds = int(seconds)

    for _ in range(seconds * 1000):
        try:
            stat = os.stat(filename)
            if stat.st_size != size:
                # file incomplete, wait for more data
                time.sleep(0.001)
                continue
        except Exception:
            # file not found, wait
            time.sleep(0.001)
            continue

        # file received, check content
        assert md5sum(filename) == h

        # ok => delete and quit
        #os.unlink(filename)
        return

    # loop stops before receiving file
    raise Exception('File not received')

def test_no_file(context, dir, name, seconds):
    # get info about the file
    info = context.files[name]
    size = info['size']
    h = info['hash']

    # where it should be
    filename = os.path.join(dir, name)

    # wait for it
    seconds = int(seconds)

    for _ in range(seconds * 1000):
        try:
            stat = os.stat(filename)
            if stat.st_size != size:
                # file incomplete, wait for more data
                time.sleep(0.001)
                continue
        except Exception:
            # file not found, wait
            time.sleep(0.001)
            continue

        # file received, check content
        assert md5sum(filename) == h

        # ok => delete and quit
        os.unlink(filename)
        raise Exception('File received')

    # loop stops before receiving file

@when('diode-file-send file {name} of size {size}')
def step_impl(context, name, size):

    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    send_file(context, name, count, blocksize)

@when('diode-send restarts while diode-file-send file {name} of size {size}')
def step_impl(context, name, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    send_file(context, name, count, blocksize, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_diode_send(context)
    start_diode_send(context)

@when('diode-receive restarts while diode-file-send file {name} of size {size}')
def step_impl(context, name, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    send_file(context, name, count, blocksize, True)
    # transfer is in progress, wait 1 second then restart diode
    time.sleep(3)
    stop_diode_receive(context)
    time.sleep(5)
    start_diode_receive(context)

@then('diode-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir.name, name, seconds)

@when('diode-file-receive file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_file(context, context.receive_dir.name, name, seconds)


@when('diode-file-send {files} files of size {size}')
def step_impl(context, files, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    for i in range(int(files)):
        name = str(f"test_file_{i}")
        create_file(context, name, count, blocksize)

    # now send all of them at once
    send_multiple_files(context)

@then('diode-file-receive all files in {seconds} seconds')
def step_impl(context, seconds):
    for name in context.files:
        test_file(context, context.receive_dir.name, name, seconds)

# diode-send-dir steps

@given(u'diode with send-dir is started')
def step_impl(context):
    start_diode(context)
    start_diode_send_dir(context)

@when(u'we copy a file {name} of size {size}')
def step_impl(context, name, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    temp_dir = TemporaryDirectory(dir=context.base_dir)

    filename = os.path.join(temp_dir.name, name)
    create_file(context, filename, count, blocksize)
    shutil.copy(filename, context.send_dir.name)

    temp_dir.cleanup()

@when(u'we copy {files} files of size {size}')
def step_impl(context, files, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    temp_dir = TemporaryDirectory(dir=context.base_dir)

    for i in range(int(files)):
        context.counter += 1
        name = str(f"test_file_{context.counter}_{i}")
        filename = os.path.join(temp_dir.name, name)
        create_file(context, filename, count, blocksize)
        shutil.copy(filename, context.send_dir.name)

    temp_dir.cleanup()


@when(u'we move a file {name} of size {size}')
def step_impl(context, name, size):
    # extract size & unit
    count = size[0:-2]
    blocksize = size[-2:]

    if blocksize not in ['KB', 'MB', 'GB']:
        raise Exception("Unknown unit")

    temp_dir = TemporaryDirectory(dir=context.base_dir)

    filename = os.path.join(temp_dir.name, name)
    create_file(context, filename, count, blocksize)
    destname = os.path.join(context.send_dir.name, name)
    os.rename(filename, destname)

    temp_dir.cleanup()

@then('diode-file-receive no file {name} in {seconds} seconds')
def step_impl(context, name, seconds):
    test_no_file(context, context.receive_dir.name, name, seconds)

@then(u'file {name} is in source directory')
def step_impl(context, name):
    test_file(context, context.send_dir.name, name, 1)

