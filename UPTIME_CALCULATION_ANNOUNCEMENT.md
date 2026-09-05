# Uptime Calculation Update for Light Wallet Server Monitoring

We've updated how 30-day uptime is calculated on the monitoring dashboard. This change provides a fairer representation of server reliability while accounting for how long each server has been tracked.

## What Changed

**Previous calculation (ZEC):**
- Uptime was calculated as `successful checks / max checks across all servers`
- This compared every server against whichever server had the most checks in the 30-day period
- Servers that were offline or unreachable for periods received fewer total checks, which penalized them twice: once for the failed checks, and again for having fewer checks than the most-active server

**Previous calculation (BTC):**
- Uptime was calculated as `successful checks / server's own total checks`
- This only measured success rate during the time the server was being checked

**New calculation (both networks):**
- Uptime is now calculated as `(successful checks / server's own total checks) × (days announced / 30)`
- The "days announced" factor is capped at 30 days

## What This Means

The new formula has two components:

1. **Success rate** - What percentage of checks succeeded out of the checks performed against this specific server
2. **Time factor** - How much of the 30-day window the server has been known to the monitoring system

| Server Age | Maximum Possible Uptime |
|------------|------------------------|
| 1 day | 3.33% |
| 7 days | 23.33% |
| 15 days | 50% |
| 30+ days | 100% |

## Why We Made This Change

The previous ZEC calculation had a flaw: comparing against the most-checked server meant that uptime scores depended on the behavior of other servers in the network. A server with 100% success rate could still show reduced uptime simply because another server had accumulated more checks.

The new approach:
- Evaluates each server independently based on its own check history
- Prevents newly announced servers from immediately achieving high uptime scores
- Rewards consistent, long-term availability
- Uses the same formula for all networks

## Impact on Existing Servers

- Servers tracked for 30+ days with high success rates may see their uptime scores increase (no longer penalized by comparison to other servers)
- Newer servers will see lower uptime percentages until they reach the 30-day threshold
- ZEC and BTC now use identical uptime calculations

If you have questions about this change, please reply to this thread.
