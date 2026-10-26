import socket, sys
path, out = sys.argv[1], sys.argv[2]
srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(path)
srv.listen(8)
open(out, "ab").close()
print("listening", flush=True)
while True:
    conn, _ = srv.accept()
    with conn:
        while True:
            data = conn.recv(65536)
            if not data:
                break
            with open(out, "ab") as f:
                f.write(data)
