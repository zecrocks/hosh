import struct
import binascii
import datetime

def parse_block_header(header_hex):
    # Convert hex to bytes
    header = bytes.fromhex(header_hex)

    # Unpack the header fields
    version = struct.unpack('<I', header[0:4])[0]
    prev_block = header[4:36][::-1].hex()  # Reverse bytes for little-endian
    merkle_root = header[36:68][::-1].hex()  # Reverse bytes for little-endian
    timestamp = struct.unpack('<I', header[68:72])[0]
    bits = struct.unpack('<I', header[72:76])[0]
    nonce = struct.unpack('<I', header[76:80])[0]

    # Convert timestamp to human-readable date
    timestamp_human = datetime.datetime.utcfromtimestamp(timestamp)

    # Return parsed data
    return {
        "version": version,
        "prev_block": prev_block,
        "merkle_root": merkle_root,
        "timestamp": timestamp,
        "timestamp_human": timestamp_human,
        "bits": bits,
        "nonce": nonce
    }

