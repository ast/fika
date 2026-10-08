{
  description = "fika: an HF group-chat digital mode for amateur radio (SM6WJM)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forAllSystems (
        system: pkgs: {
          default = pkgs.mkShell {
            packages = [
              # Rust toolchain
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.rust-analyzer

              # cpal needs ALSA headers at build time
              pkgs.pkg-config
              pkgs.alsa-lib

              # hamlib ships rigctld, which fika talks to for PTT and frequency
              pkgs.hamlib

              # task runner and audio utilities for test recordings
              pkgs.just
              pkgs.sox
            ];

            shellHook = ''
              echo "fika: rustc $(rustc --version | cut -d' ' -f2)  |  $(rigctld --version 2>&1 | head -1)"
            '';
          };
        }
      );
    };
}
