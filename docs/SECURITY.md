# Security

DarwinPlay executes Windows software and treats imported executables and runtime archives as untrusted input.

## DarwinWine artifacts

DarwinPlay accepts only DarwinWine runtime archives selected explicitly by the user. Before activation it:

- lists archive contents and rejects absolute paths and parent traversal;
- extracts into a private staging directory;
- requires schema-2 `runtime.json` identifying `DarwinWine` and an `x86_64` architecture;
- validates relative entrypoint paths;
- requires the declared `wine` and `wineserver` files;
- checks the runtime-reported Wine version against the manifest;
- creates a disposable prefix with `wineboot`;
- activates the runtime atomically only after the probe succeeds.

DarwinPlay does not execute Homebrew installers, search PATH for another Wine, invoke Sikarugir, or accept an arbitrary Wine executable override.

DarwinWine source/build security belongs to the separate DarwinWine repository. DarwinPlay consumes only its packaged output.

## Prefix isolation

The runtime removes Wine's default `Z:` mapping from managed prefixes. Imported games receive explicit directory mappings. Steam setup receives only a temporary installer mapping.

## Credentials

Steam authentication remains inside the Windows Steam client. DarwinPlay does not collect, proxy, persist or inspect Steam credentials.

## Graphics components

DXMT is staged and validated separately from DarwinWine. Installing or removing DXMT must not mutate the DarwinWine runtime artifact.
