# 0022: The desktop app reports its own work

- PR: [TrogonStack/t3code#35](https://github.com/TrogonStack/t3code/pull/35)
- Status: active

## What you can do now

- See what the desktop app itself is doing. App startup, window and menu work,
  backend supervision, and updates now reach your collector as traces, logs,
  and metrics under the service name `desktop`, alongside the server work they
  cause.
- Configure it the way you configure everything else. The Electron main process
  reads the same `OTEL_*` endpoint variables as the server, in the same order,
  so a machine that points one of them at a collector points both. It had a
  trace exporter before, and only its own `T3CODE_OTLP_TRACES_URL` could reach
  it, which almost nobody sets.
- Get logs and metrics from it, not only traces. A crash loop before the server
  is even up used to leave nothing behind but a local file on the machine it
  happened on.
- Turn it off the same way. `OTEL_SDK_DISABLED=true` stops both processes.
- Tell the two apart without trusting the environment. The main process reports
  as `t3-desktop`, joining `t3-server` and `t3-web`, and `service.runtime` on it
  is always `desktop`, so an ambient
  `OTEL_RESOURCE_ATTRIBUTES=service.runtime=t3-server` cannot make it file its
  work under the server's name. See 0023 for why names are static.

## Why

The desktop app is two processes, and only one of them was reachable. A user who
set up a collector got the server and quietly got nothing from the process that
starts it, owns its windows, and restarts it when it dies. The most useful
telemetry the desktop app could send is about the minutes before the server
exists, and those were the minutes nothing was recorded.

The trace exporter that was already there is the sharper part of the story. It
was real, it worked, and it read one variable nobody sets, so it read as a
feature that had been tried and found not to help. It had not been tried.

Reading the same variables in both processes is the whole point. A telemetry
variable that half an app honors is worse than one it ignores entirely, because
the half that arrives looks like the whole.

## Upstream considerations

This belongs upstream and depends on 0018 being there first: it is the same
environment reading applied to the other process, which is why the reading moved
into a shared package instead of being copied. Upstream taking 0018 gets this
almost for free.

The rebase burden is a single assembly point. The main process gets one logger
set for its lifetime, so the OTLP log exporter has to be built in the same call
that builds the console logger rather than merged in beside it. A sync that
rewrites that assembly and splits them apart will silently drop either the
console output or the export.
