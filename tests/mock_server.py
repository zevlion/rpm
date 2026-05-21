import os
import sys
import socket
import time

port = int(os.environ.get("PORT", "9000"))
pid = os.getpid()

server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.listen(5)

print(f"Mock server running on port {port} with pid {pid}", flush=True)

# Keep allocating memory array in global scope so it doesn't get GC'd
leak = []

while True:
    try:
        conn, addr = server.accept()
        data = conn.recv(1024).decode('utf-8', errors='ignore')
        if "allocate_memory" in data:
            # Allocate 150MB of memory to exceed the 50MB limit
            leak.append(bytearray(150 * 1024 * 1024))
            conn.sendall(f"allocated memory on pid {pid}\n".encode('utf-8'))
        elif "exit" in data:
            conn.sendall(f"exiting pid {pid}\n".encode('utf-8'))
            conn.close()
            break
        else:
            conn.sendall(f"hello from port {port} pid {pid}\n".encode('utf-8'))
        conn.close()
    except Exception as e:
        print(f"Error: {e}", flush=True)
        break
