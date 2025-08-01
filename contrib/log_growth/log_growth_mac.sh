#!/bin/bash

# Function to format bytes into human readable format
format_bytes() {
    local bytes=$1
    # Convert to integer for comparison
    local bytes_int=${bytes%.*}
    if [[ $bytes_int -gt 1073741824 ]]; then
        echo "$(echo "scale=1; $bytes / 1073741824" | bc)G"
    elif [[ $bytes_int -gt 1048576 ]]; then
        echo "$(echo "scale=1; $bytes / 1048576" | bc)M"
    elif [[ $bytes_int -gt 1024 ]]; then
        echo "$(echo "scale=1; $bytes / 1024" | bc)K"
    else
        echo "${bytes}B"
    fi
}

# Function to get actual log file size if accessible
get_log_size() {
    local container_id=$1
    local log_path=$(docker inspect --format '{{.LogPath}}' "$container_id" 2>/dev/null)
    
    if [[ -n "$log_path" ]]; then
        # Try to get size using docker exec (works if container is running)
        local size=$(docker exec "$container_id" sh -c "wc -c < $log_path 2>/dev/null" 2>/dev/null)
        if [[ -n "$size" && "$size" != "0" ]]; then
            echo "$size"
            return 0
        fi
    fi
    
    # Fallback: estimate from docker logs
    local log_lines=$(docker logs --tail 1000 "$container_id" 2>/dev/null | wc -l)
    if [[ $log_lines -gt 0 ]]; then
        # Rough estimate: 150 bytes per line (more realistic)
        echo $((log_lines * 150))
        return 1  # Indicate this is an estimate
    fi
    
    echo "0"
    return 2  # No logs found
}

echo "Container Log Growth Analysis"
echo "============================"
echo ""

for id in $(docker ps -aq); do
  name=$(docker inspect --format '{{.Name}}' $id | cut -c2-)
  
  # Get container start time
  start=$(docker inspect --format '{{.State.StartedAt}}' $id)
  
  # Parse ISO 8601 date format for macOS date
  # Remove timezone info and microseconds
  start_clean=$(echo "$start" | sed 's/\.[0-9]*Z//' | sed 's/\.[0-9]*+[0-9]*:[0-9]*//')
  start_epoch=$(date -j -f "%Y-%m-%dT%H:%M:%S" "$start_clean" +%s 2>/dev/null || echo 0)
  
  current_epoch=$(date +%s)
  uptime_seconds=$((current_epoch - start_epoch))
  uptime_days=$(echo "scale=2; $uptime_seconds / 86400" | bc)
  
  if (( $(echo "$uptime_days > 0" | bc -l) )); then
    # Get log size
    log_size=$(get_log_size "$id")
    is_estimate=$?
    
    if [[ $log_size != "0" ]]; then
      rate=$(echo "scale=2; $log_size / $uptime_days" | bc)
      
      if [[ $is_estimate -eq 1 ]]; then
        echo "$name: ~$(format_bytes $log_size) estimated, ~$(format_bytes $rate)/day (estimated)"
      else
        echo "$name: $(format_bytes $log_size) actual, ~$(format_bytes $rate)/day"
      fi
    else
      echo "$name: no logs found"
    fi
  else
    echo "$name: too new to calculate (< 1 day)"
  fi
done

echo ""
echo "Note: On macOS, actual log files are stored inside Docker Desktop VM."
echo "Use 'docker logs <container>' to view logs directly."