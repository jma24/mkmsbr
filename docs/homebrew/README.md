# Homebrew tap maintenance

The mkmsbr Homebrew formula lives in a separate tap repo:
[jma24/homebrew-mkmsbr](https://github.com/jma24/homebrew-mkmsbr). This
directory holds the developer-facing process docs for releasing new
versions through the tap.

## Verify a formula locally before publishing

From a checkout of the tap repo:

```sh
brew install --build-from-source ./Formula/mkmsbr.rb
brew test ./Formula/mkmsbr.rb
brew audit --strict --online ./Formula/mkmsbr.rb
```

`brew audit` will flag any style or metadata issues; fix and recommit
before pushing the tap.

## User install path

```sh
brew tap jma24/mkmsbr
brew install mkmsbr
```

Or one-shot:

```sh
brew install jma24/mkmsbr/mkmsbr
```

## Updating for a new release

For each new tagged release `vX.Y.Z`:

1. Bump `url` in the tap repo's `Formula/mkmsbr.rb` to the new tag tarball.
2. Recompute the sha256:
   ```sh
   curl -sL https://github.com/jma24/mkmsbr/archive/refs/tags/vX.Y.Z.tar.gz \
     | shasum -a 256
   ```
3. Update `sha256` in `Formula/mkmsbr.rb`.
4. Commit + push to the tap repo. `brew update && brew upgrade mkmsbr`
   then picks it up for users.

## Notes

- The formula builds from source (`cargo install`). v1.0.1 ships
  pre-assembled boot-code blobs in `blobs-prebuilt/`, so the build does
  **not** require nasm on the user's host.
- No bottles (pre-built binaries) for now. If demand warrants it later,
  add a GitHub Actions workflow that uploads bottled binaries as release
  assets and reference them with a `bottle do ... end` block in the
  formula.
