# Artifact Executor

An artifact-oriented exectution environment written in [BASH](https://www.gnu.org/software/bash/) 5.2.1. The project relies heavily on "standard" Linux commandline tools such as `grep`, `sort`, and `sha256sum`.

This project is similar to [bazel](https://bazel.build/): it executes binaries in a best-effort "sandboxed" environment, requiring users to specify their environment variables, input files, and output files. Environment variables, inputs, and ouputs are cached, and re-execution is skipped (and cached outputs are copied into place) when it appears that no dependencies have changed.

## Caveats

- This project is _much, much_ slower and less mature than [bazel](https://bazel.build/);

- Most path handling is done in terms of absolute paths, so running on an identical copy of inputs that are located in a different directory _may_ cause steps to be re-executoed
