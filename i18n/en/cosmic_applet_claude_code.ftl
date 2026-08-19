applet-name = Claude Code
no-sessions = No active sessions
sessions = { $count ->
    [one] { $count } session
   *[other] { $count } sessions
}
status-busy = working
status-idle = idle
five-hour = 5-hour limit
seven-day = Weekly limit
resets-in = resets in { $time }
resets-at = resets { $time }
context = { $percent }% context
no-limits = Usage data unavailable
no-limits-hint = Start a Claude Code session, or run `just install-bridge` to enable the status line bridge.
captured-ago = updated { $time } ago
open-settings = Panel settings
unknown = unknown
weekday-mon = Mon
weekday-tue = Tue
weekday-wed = Wed
weekday-thu = Thu
weekday-fri = Fri
weekday-sat = Sat
weekday-sun = Sun
