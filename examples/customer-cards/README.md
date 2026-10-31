# Customer Cards

Your customers as cards, with search: an example of an app with an interface of its own and no
plugin, the way a third party ships one.

* `app.yaml` declares the interface (`ui`, `shell.app`) and what its pages may reach (`access`:
  reading customers). Installing it asks the administrator for exactly that.
* `ui/` is the interface: plain HTML, a module script and a stylesheet, no build step.
  `ui/ferrum.js` is the bridge to Ferrum; copy it into your own app.
* Uploaded unsigned to one workspace (`ferrum-app dev examples/customer-cards --install` against a
  server with `FERRUM_APP_UPLOADS=private`), the interface runs isolated. Its "How it is kept
  apart" panel shows what that means: no session, no storage of the workspace's site, records it
  did not ask for refused, and the rest of the API out of reach.
