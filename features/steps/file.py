
import hashlib
import os
import time


def parse_human_size(size):
    """Parse file size string into bytes."""
    # Handle special case of 0B
    if size == "0B":
        return 0

    # Extract size & unit
    count = int(size[0:-2])
    unit = size[-2:]

    if unit == 'KB':
        size_in_bytes = count * 1024
    elif unit == 'MB':
        size_in_bytes = count * 1024 * 1024
    elif unit == 'GB':
        size_in_bytes = count * 1024 * 1024 * 1024
    else:
        raise Exception("Unknown unit")

    return size_in_bytes

def md5sum(filename, blocksize=65536):
    """Calculate MD5 hash of a file."""
    if not os.path.exists(filename):
        raise FileNotFoundError(f"File {filename} does not exist")
    h = hashlib.md5()
    with open(filename, "rb") as f:
        for block in iter(lambda: f.read(blocksize), b""):
            h.update(block)
    return h.hexdigest()

def create_file(context, filename, size):
    """Create a file with specified size using Python."""
    file_size = parse_human_size(size)

    # Use Python's open() to write random data directly
    with open(filename, 'wb') as f:
        remaining = file_size
        while remaining > 0:
            chunk_size = min(1024 * 1024, remaining)  # 1MB chunks
            chunk = os.urandom(chunk_size)
            f.write(chunk)
            remaining -= chunk_size

    # Verify file exists and has correct size
    if not os.path.exists(filename):
        print(f"DEBUG: File {filename} does not exist after write")
        print(f"DEBUG: Directory contents: {os.listdir(os.path.dirname(filename))}")
        raise Exception(f"File {filename} was not created")

    actual_size = os.path.getsize(filename)

    if actual_size != file_size:
        raise Exception(f"File {filename} has wrong size: {actual_size} != {file_size}")

    # Store info in context
    store_file_info(context, filename)

def store_file_info(context, filename):
    """Store file information in the context."""
    # Verify file exists before accessing it
    if not os.path.exists(filename):
        raise Exception(f"File {filename} does not exist when storing info")
    
    # Store info about the generated file in context
    try:
        file_size = os.stat(filename).st_size
    except OSError as e:
        raise Exception(f"Cannot stat file {filename}: {e}")
    
    try:
        file_hash = md5sum(filename)
    except Exception as e:
        raise Exception(f"Cannot compute hash for file {filename}: {e}")

    name = os.path.basename(filename)

    context.files[name] = { 'size': file_size, 'hash': file_hash, 'path': filename }

def wait_for_file(context, dir, name, seconds, expect_file=True):
    """Wait for a file to be received with content verification."""
    # Get info about the file
    info = context.files[os.path.basename(name)]
    file_size = info['size']
    file_hash = info['hash']

    # Where it should be
    filename = os.path.join(dir, name)

    # Wait for it
    timeout_seconds = int(seconds)
    timeout_milliseconds = timeout_seconds * 1000 + 1
    
    for _ in range(timeout_milliseconds):
        try:
            stat = os.stat(filename)
            if stat.st_size != file_size:
                # File incomplete, wait for more data
                time.sleep(0.001)
                continue
        except (FileNotFoundError, OSError):
            # File not found, wait
            time.sleep(0.001)
            continue

        # File size matches, compute hash to verify content
        try:
            file_content_hash = md5sum(filename)
        except Exception:
            # Hash computation failed, file might still be writing
            time.sleep(0.001)
            continue
        
        # Verify hash matches
        if file_content_hash != file_hash:
            raise Exception(f'File content hash mismatch for {name}: expected {file_hash}, got {file_content_hash}')

        # File received and verified
        if expect_file:
            return
        else:
            os.unlink(filename)
            raise Exception('File received')

    # Loop stops before receiving file
    if expect_file:
        raise Exception(f'File {filename} not received')

def test_file(context, dir, name, seconds):
    """Test that a file is received."""
    wait_for_file(context, dir, name, seconds, expect_file=True)

def test_no_file(context, dir, name, seconds):
    """Test that a file is not received."""
    wait_for_file(context, dir, name, seconds, expect_file=False)

def create_and_copy_file(context, name, size):
    """Create a file and copy it to the send directory."""
    send_dir = context.send_dir
    filename = os.path.join(send_dir, name)
    create_file(context, filename, size)

def create_and_copy_multiple_files(context, files, size):
    """Create multiple files and copy them to the send directory."""
    send_dir = context.send_dir
    import time
    suffix = int(time.time() * 1000) % 10000
    for i in range(int(files)):
        name = str(f"test_file_{suffix}_{i}")
        filename = os.path.join(send_dir, name)
        create_file(context, filename, size)

def create_and_move_file(context, name, size):
    """Create a file and move it to the send directory."""
    tmpname = os.path.join(context.tmp_dir, name)
    destname = os.path.join(context.send_dir, name)
    create_file(context, tmpname, size)
    os.rename(tmpname, destname)


def create_test_dir_in_place(context, name, size):
    """Create the same dir hierarchy as create_and_move_test_dir(), but
    directly in the send directory (no tmp dir + rename), so the entries
    are already present before lidi-dir-send starts and can only be picked
    up by its initial directory scan, never by an inotify event.
    """
    destname = os.path.join(context.send_dir, name)
    dir_a = os.path.join(destname, ".A")
    dir_b = os.path.join(destname, "B")
    dir_bb = os.path.join(dir_b, "BB")
    file_c = os.path.join(destname, ".C")
    file_aa = os.path.join(dir_a, "AA")
    os.makedirs(destname)
    os.mkdir(dir_a)
    os.makedirs(dir_bb)
    create_file(context, file_aa, "0B")
    create_file(context, file_c, size)


def create_and_move_test_dir(context, name, size):
    """Create a dir hierarchy and move it to the send directory.
    The hierarchy contains an empty and a non-empty dir, + an empty and a non-empty file.
    One of the files and one of the dirs start with a dot to test ignore rules.
    """
    tmpname = os.path.join(context.tmp_dir, name)
    destname = os.path.join(context.send_dir, name)
    dir_a = os.path.join(tmpname, ".A")
    dir_b = os.path.join(tmpname, "B")
    dir_bb = os.path.join(dir_b, "BB")
    file_c = os.path.join(tmpname, ".C")
    file_aa = os.path.join(dir_a, "AA")
    os.makedirs(tmpname)
    os.mkdir(dir_a)
    os.makedirs(dir_bb)
    create_file(context, file_aa, "0B")
    create_file(context, file_c, size)
    os.rename(tmpname, destname)


def wait_for_dir(context, name, seconds, expect_dir):
    """Wait for a directory to be received and verify its content"""
    path = os.path.join(context.receive_dir, name)

    timeout_seconds = int(seconds)
    timeout_milliseconds = timeout_seconds * 1000

    for _ in range(timeout_milliseconds):
        if not os.path.isdir(path):
            time.sleep(0.001)
            continue

        if not expect_dir:
            raise Exception(f'Directory {path} unexpectedly received')

        dir_a = os.path.join(path, ".A")
        dir_bb = os.path.join(path, "B", "BB")
        if (not os.path.isdir(dir_a)
                or not os.path.isdir(dir_bb)):
            raise Exception(f'Missing a dir in {path}')

        wait_for_file(context, dir_a, "AA", 0, True)
        wait_for_file(context, path, ".C", 0, True)
        return

    if expect_dir:
        raise Exception(f'Directory {path} not received')
