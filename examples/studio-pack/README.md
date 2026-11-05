# Fitness Studio

An example of an industry pack in metadata alone (`runtime: meta`): the records a fitness studio
keeps, who may do what with them, a membership's approval, and the plans it starts with. Ferrum gives
every record its lists, forms, search, permissions, history, exports and the REST and MCP APIs, so
the studio's staff and AI agents work with them under the same rules.

- `packages/studio_pack/entities/studio.yaml`: members, plans, memberships, classes and bookings.
- `package.yaml`: two roles (Front Desk, Studio Manager); the Membership Approval workflow, in which
  the desk asks and the manager approves; and three plans to start from (seed records, kept by key).

Check it with the platform's packages, then try it on a development server:

    ferrum-app check examples/studio-pack --platform-packages ../Ferrum/packages
    ferrum-app dev examples/studio-pack --install

A pack that needs behaviour beyond metadata adds a plugin (`runtime: wasm`) or a service of its own
(`runtime: service`, as `examples/ticket-tagger` does).
