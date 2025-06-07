{
  description = "Iori";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      naersk,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        naersk-lib = pkgs.callPackage naersk { };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };
      in
      {
        packages.default = naersk-lib.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [
            pkg-config
            rustPlatform.bindgenHook
            ffmpeg.dev
          ];
          buildInputs = with pkgs; [
            protobuf
          ];
          cargoBuildOptions = opts: opts ++ [ "--workspace" ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            pkg-config
            rustPlatform.bindgenHook
            ffmpeg.dev
            protobuf

            mkvtoolnix-cli
          ];
        };
      }
    );
}
