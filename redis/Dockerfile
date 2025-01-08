# Use the official Redis image as a base
FROM redis:7.2-alpine

# Expose the default Redis port
EXPOSE 6379

# Provide a default Redis configuration (optional)
# Copy a custom Redis configuration file if needed
# COPY redis.conf /usr/local/etc/redis/redis.conf

# Uncomment the line below to start Redis with a custom config file
# CMD ["redis-server", "/usr/local/etc/redis/redis.conf"]

# Default command to run Redis server
CMD ["redis-server"]
