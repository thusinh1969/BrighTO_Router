# Visual references for DeepSeek - 2026-09-17

User asked for professional visuals so DeepSeek can use them as design direction.

Created under:

```text
swarm/docs_builds/visuals/
```

Files:

```text
github-banner.png          public GitHub/README banner direction
github-banner.svg          editable SVG draft
portal-admin-mock.png      Admin Portal visual direction
portal-admin-mock.svg      editable SVG draft
portal-user-mock.png       User Portal visual direction
portal-user-mock.svg       editable SVG draft
benchmark-visual.png       benchmark strategy visual direction
benchmark-visual.svg       editable SVG draft
README.md                  short usage map for the assets
```

Implementation guidance:

- Use the PNGs as visual target; they are the reviewed previews.
- Keep the real Portal static HTML/CSS/JS unless a concrete product reason requires a build system.
- Admin Portal must clearly show provider setup, route creation, teams/API keys, usage, and safe key handling.
- User Portal must clearly separate user API-key login from admin master-key access.
- Benchmark visual should support the docs message: same machine, same payload, router path compared with direct backend path; no fake pass/fail targets.
- Do not copy provider plaintext keys into UI state, logs, screenshots, or API responses.
