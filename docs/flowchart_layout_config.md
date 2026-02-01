# Flowchart Layout Config

This documents the new flowchart layout tuning options added for the layout overhaul.

## Config keys (JSON / init directives)

These can be set under `flowchart` in config files or `%%{init}%%` directives.

```json
{
  "flowchart": {
    "orderPasses": 4,
    "portPadRatio": 0.2,
    "portPadMin": 4,
    "portPadMax": 12,
    "portSideBias": 0.0
  }
}
```

## Field meaning

- `orderPasses`: number of forward/backward barycenter passes during manual ordering.
- `portPadRatio`: fraction of node size reserved as padding on a port side.
- `portPadMin`: minimum padding in pixels.
- `portPadMax`: maximum padding in pixels.
- `portSideBias`: extra offset applied per-port to spread ports when many share a side.
