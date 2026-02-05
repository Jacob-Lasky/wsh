See @docs/VISION.md for project vision and purpose.

# Additional Instructions
- Project is under active development. Do not criticize missing features.
- Project is being developed with Nix. You **must** wrap all `cargo`
  commands: `nix develop -c sh -c "cargo check"`, etc.
- When implementing new features or making material refactors, ensure
  that unit tests are in place.
- When finishing an implementation phase, ensure integration and/or
  end-to-end tests are in place.
