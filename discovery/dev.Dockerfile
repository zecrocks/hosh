FROM hosh/dev

WORKDIR /usr/src/app

# Use cargo-watch with improved options for better development experience
CMD ["cargo", "watch", "-q", "-c", "-w", ".", "-x", "run"] 