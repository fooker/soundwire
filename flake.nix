{
  description = "Sound routing daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (crane.mkLib pkgs).overrideToolchain rust;

        src = craneLib.cleanCargoSource (craneLib.path ./.);

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            alsa-lib
          ];
        };

        soundwire = (craneLib.buildPackage {
          inherit src;

          strictDeps = true;

          inherit cargoArtifacts;

          buildInput = with pkgs; [
            alsa-lib
          ];
        });
      in
      {
        checks = {
          inherit soundwire;
        };

        packages = rec {
          inherit soundwire;
          default = soundwire;
          deps = cargoArtifacts;
        };

        apps = rec {
          soundwire = flake-utils.lib.mkApp {
            drv = pkgs.writeShellScriptBin "soundwire" ''
              ${soundwire}/bin/soundwire
            '';
          };
          default = soundwire;
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          inputsFrom = [ cargoArtifacts soundwire ];

          buildInputs = with pkgs; [
            cargo-deny
            cargo-outdated
          ];

          RUST_BACKTRACE = 1;
          RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
        };
      });
}
