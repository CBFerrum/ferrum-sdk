# Ticket Tagger

An example of an app whose code runs on your own servers (`runtime: service`, Ferrum's docs/plugins.md,
"Services on their own servers"), and of an AI tool built that way: it hears of each new Helpdesk
ticket, works out its topic (from its words, or by asking a model) and sets it on the ticket.

- `app.yaml` says where the service is (`service.url`), what it hears of (`Ticket.created`) and what
  its account may do (`access`: read tickets and set their topic). The install dialog shows all three.
- `packages/ticket_tagger` adds the Topic field to tickets.
- `service/tagger.py` is the service, in Python's standard library: it keeps the key and the secret
  Ferrum sends to `install`, checks each event's signature, reads the ticket with its key (an account
  that reaches tickets and nothing else) and sets the topic, against the ticket's version.

Run it, then point `service.url` at it and install the app:

    TAGGER_PORT=8090 python3 service/tagger.py
    python3 -m unittest discover service

With `ANTHROPIC_API_KEY` set it asks a model (`TAGGER_MODEL`, default `claude-haiku-5-5`); a
ticket's subject and description are all it sends. Without one it goes by the ticket's words.

A server reaches a service over HTTPS; plain `http://` (as above, on your own machine) works only
on a server that allows private addresses, for development. A store lists services at an https
address only.
