# Third-party runtime components

DarwinPlay itself is licensed under the repository LICENSE.

## DarwinWine / Wine

DarwinPlay supports only packaged DarwinWine runtime artifacts produced by the separate DarwinWine repository. DarwinWine is based on Wine, which is licensed under LGPL-2.1-or-later. DarwinPlay does not include DarwinWine or Wine source code in this source archive.

The current minimum supported runtime is DarwinWine cx26.3-dp5 for x86_64 macOS.

## zstd

The `darwinplay-runtime` binary statically links libzstd (via the Rust `zstd` crate) to unpack DarwinWine runtime archives. Zstandard is copyright Meta Platforms, Inc. and affiliates, licensed under the BSD 3-Clause License (https://github.com/facebook/zstd/blob/dev/LICENSE).

## Steam

Steam is not distributed with DarwinPlay. DarwinPlay downloads Valve's official Windows Steam bootstrapper when the user requests Steam installation.
