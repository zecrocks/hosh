# computes growth rate of container logs in a linux environment

for id in $(docker ps -aq); do
  name=$(docker inspect --format '{{.Name}}' $id | cut -c2-)
  log=$(docker inspect --format '{{.LogPath}}' $id)
  size=$(du -b "$log" | cut -f1)
  start=$(docker inspect --format '{{.State.StartedAt}}' $id)
  uptime_days=$(echo "scale=2; ($(date +%s) - $(date -d "$start" +%s)) / 86400" | bc)
  if (( $(echo "$uptime_days > 0" | bc -l) )); then
    rate=$(echo "scale=2; $size / $uptime_days" | bc)
    echo "$name: $(numfmt --to=iec $size) total, $(numfmt --to=iec $rate)/day"
  else
    echo "$name: too new to calculate"
  fi
done