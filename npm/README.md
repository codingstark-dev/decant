# decant-cli

`decant-cli` installs the `decant` command from the Decant GitHub release for your platform.

```bash
npm install -g decant-cli
decant clone https://example.com --output ./capture
```

Decant mirrors a website's HTML, CSS, JS, fonts, images, screenshots, and design tokens into a local capture that works offline and can be handed to AI agents.

For JS-heavy sites, install a release with render support and run Chrome mode:

```bash
decant clone https://example.com --render chrome --runtime-capture auto --output ./capture
```

Chrome runtime capture saves browser-observed static resources such as dynamic modules, images, stylesheets, fonts, media, and manifests. Live backend state, authenticated sessions, WebSocket/SSE streams, protected media, and server-personalized APIs are not fully capturable as static offline files.
