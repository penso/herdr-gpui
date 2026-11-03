#!/usr/bin/env python3
"""Drive a sandbox herdr-remote-control daemon through its control socket,
as Herdr UI's pairing dialog does, for the simulator loop.

    remote_control.py STATE_DIR pair      # prints the pairing link
    remote_control.py STATE_DIR approve   # approves the claimed pairing

Frames are little-endian u32 lengths then JSON (herdr_chat_protocol::frame).
"""

import json
import pathlib
import socket
import struct
import sys
import time


def request(state_dir: pathlib.Path, message: dict) -> dict:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(state_dir / "control.sock"))
        body = json.dumps(message).encode()
        sock.sendall(struct.pack("<I", len(body)) + body)
        header = sock.recv(4, socket.MSG_WAITALL)
        (length,) = struct.unpack("<I", header)
        data = b""
        while len(data) < length:
            data += sock.recv(length - len(data))
        return json.loads(data)


def main() -> int:
    state_dir = pathlib.Path(sys.argv[1])
    pairing_file = state_dir / "sim-pairing.json"
    if sys.argv[2] == "pair":
        offer = request(state_dir, {"request": "pair"})
        pairing_file.write_text(json.dumps(offer))
        print(offer["url"])
        return 0
    if sys.argv[2] == "approve":
        pairing = json.loads(pairing_file.read_text())["id"]
        for _ in range(120):
            state = request(state_dir, {"request": "pairing", "id": pairing})
            if state.get("state") == "claimed":
                answer = request(state_dir, {"request": "approve", "id": pairing, "allow": True, "code": state["code"]})
                print(f"approved {state['device_name']} with code {state['code']}: {answer}")
                return 0
            time.sleep(0.5)
        print(f"never claimed: {state}", file=sys.stderr)
        return 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
