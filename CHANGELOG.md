# Changelog
All notable changes to this project will be documented in this file.

## UNRELEASED

- Added the `StringStash` stash type
- Updated dependencies

## v0.3.1 (2025-10-21)

- Added support for flattening nested ErrorLists when using `StashableResult`
  with `BoxedErrors`.
- Renamed the `BoxedErrors` alias to `BoxedErrorList` to improve disambiguation
  with the `BoxedError` alias.
- Fixed Github Actions CD configuration.
- Documentation additions.

## v0.2.1 (2025-10-19)

- First public release. Includes the `ErrorStash` trait, `BoxedStash` and `TypedStash`
  implementations, the `ErrorList` wrapper error type, and the `StashableResult` and
  the `StashErrorsIter` extension traits.