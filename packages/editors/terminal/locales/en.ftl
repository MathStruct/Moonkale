extension-name = Terminal
extension-description = Shell sessions (local PTY on desktop, server relay on web).
terminal-title = Terminal
terminal-unavailable = Terminals are not available on this platform
terminal-failed = Could not start a terminal: { $error }
terminal-close = Close
terminal-new = New terminal
terminal-trace-title = Parse the stack traces / compiler errors in this terminal and draw them in the Graph panel
terminal-trace = Trace → Graph
terminal-no-trace = No stack trace or compiler error found in this terminal
terminal-drew =
    Drew { $n ->
        [one] { $n } trace
       *[other] { $n } traces
    }; see the Graph panel's source picker
