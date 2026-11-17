# Board

An example of an extension point (docs/plugins.md in Ferrum, "Extension points"): a meta app with
one entity, cards, that offers `board.card`, a point other apps contribute kinds of cards to. An
app that requires `board` adds one under `contributes`:

```yaml
contributes:
  board.card:
    - {id: bug, label: Bug}
```

    ferrum-app dev examples/board --install
