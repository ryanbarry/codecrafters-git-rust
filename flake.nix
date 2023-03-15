{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, utils, naersk, nixpkgs-mozilla }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [
            (import nixpkgs-mozilla)
          ];
        };

        toolchain = (pkgs.rustChannelOf {
          rustToolchain = ./rust-toolchain;
          sha256 = "sha256-AoqjoLifz8XrZWP7piauFfWCvhzPMLKxfv57h6Ng1oM=";
        }).rust.override (old: {
          extensions = [ "rust-src" "rust-analysis" ];
        });

        naersk-lib = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ];
        };

        defaultApp = utils.lib.mkApp {
          drv = self.defaultPackage."${system}";
        };

        devShell = with pkgs; mkShell {
          nativeBuildInputs = [ pkg-config ];
          buildInputs = [ openssl toolchain rustPackages.clippy ];
          RUST_LOG = "trace";
          shellHook = ''
          alias mygit="! [[ "\$\(pwd\)" == \"/tmp/testing\" ]] && echo \"unsafe! not in /tmp/testing\" || $PWD/your_git.sh"
          '';
        };
      });
}
