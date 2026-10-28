# {{name}}

{{name}} keeps notes with a status for your team: every note has a title, details and an
Open/Done status, and lives under the {{name}} menu.

## What it adds

- **Note** records (`{{entity}}`), listed and filtered by status
- The **{{role}}** role, for the people who work with them
{{plugin_summary}}

## Development

This is a Ferrum app ({{runtime}} runtime). It needs the `ferrum-app` CLI from
[ferrum-sdk](https://github.com/CBFerrum/ferrum-sdk).

    ferrum-app check                  # validate the manifest, packages and plugin
    ferrum-app dev --server http://127.0.0.1:5150 --token "$FERRUM_TOKEN" --watch

{{build_section}}
## Release

    ferrum-app keygen -o publisher.pk8          # once; register the printed key with the store
    ferrum-app pack -o {{id}}-0.1.0.fapp
    ferrum-app sign {{id}}-0.1.0.fapp --key publisher.pk8
    ferrum-app publish {{id}}-0.1.0.fapp --store "$FERRUM_STORE" --token "$FERRUM_STORE_TOKEN"

Keep `publisher.pk8` secret and out of version control.
