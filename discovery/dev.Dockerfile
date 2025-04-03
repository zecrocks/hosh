FROM hosh/dev

WORKDIR /usr/src/app

# Use cargo-watch with specific options:
# -q: Quiet mode (less output)
# -c: Clear screen between runs
# -w: Watch only specific directories
# -x: Execute command
CMD ["cargo", "watch", "-q", "-c", "-w", "src", "-x", "run"] 