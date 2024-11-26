{
  inputs = {
    nixpkgs-unstable.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs-unstable,
    rust-overlay,
    utils,
    ...
  }:

  utils.lib.eachDefaultSystem (system:
    let
      overlays = [(import rust-overlay)];

      pkgs = import nixpkgs-unstable {
        inherit system overlays;

        # Best practice - avoids allowing impure options set by default.
        config = {};
      };

      # Additional packages required for linking Rust objects on Darwin.
      systemSpecificPkgs = if pkgs.stdenv.isDarwin then [
          pkgs.darwin.apple_sdk.frameworks.Security
          pkgs.darwin.libiconv
        ] else [];
    in {
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; systemSpecificPkgs ++ [
          # Rust
          rust-bin.stable."1.81.0".default
          rust-bin.stable."1.81.0".rust-analyzer
        ];
      };

      formatter.${system} = pkgs.alejandra;
    }
  );
}
