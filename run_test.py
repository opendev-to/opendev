import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    if line.startswith("Content-Length:"):
        length = int(line.split(":")[1].strip())
        sys.stdin.readline()  # empty line
        body = sys.stdin.read(length)
        response = body
        header = f"Content-Length: {len(response)}\r\n\r\n"
        sys.stdout.write(header)
        sys.stdout.write(response)
        sys.stdout.flush()
