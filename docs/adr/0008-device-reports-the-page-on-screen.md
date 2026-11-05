# The Device reports the Page on screen with a `v2.page` event

Home's Device miniature and `DeviceSummary.layout` need the Page the Device shows.
The Firmware changes Pages on its own (a swipe, a page action, a wake to the last
Page, a Layout load), and before this the link carried none of it: only taps and
drags named their Page.

Firmware that reports Pages now sends `{"m":"v2.page","page":"<id>"}` on a selected
v2 session, once after selection and again on every Page or Layout change. The
link task compares the UI's page index and Layout load count each turn and reads
the Page id under the layout lock only when one changed, so the LVGL task never
waits on the link.

## Considered options

- **Poll the `ui state` console command.** Works on Firmware already shipped, but
  puts an `@td` line in the Device Console every poll, races a person typing
  `ui state` there, lags a swipe by the poll period and depends on a debug
  command's reply shape.
- **Ship the Layout summary without the Page.** No Firmware change, but Home keeps
  guessing Page 1, which is the bug people see.
- **An event from the Firmware.** Small, pushes each change once, and needs no
  polling.

## Consequences

There is no hello field or capability. A Companion treats the Page as unknown
until the first `v2.page` of a session, so Firmware without the event shows no
Page and keeps working. Older Companions pass the unknown event to Device event
subscribers and otherwise ignore it. The event names the Page by id and the
Companion resolves it against the Layout it read, so a report from a replaced
Layout never selects the wrong Page. A lost event leaves the previous Page until
the next change or session; nothing is acknowledged or replayed.
