{
  description = "Fast, memory-efficient file search utility with predictable resource usage";

  inputs = {
    nixpkgs.url =
      "github:NixOS/nixpkgs?rev=a47b881e04af1dd6d414618846407b2d6c759380";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        # manifest = pkgs.lib.importTOML ./Cargo.toml;
        # package = manifest.package;
        # snapfind = pkgs.rustPlatform.buildRustPackage {
        #   pname = package.name;
        #   version = package.version;
        #   src = pkgs.lib.cleanSource ./.;
        #   cargoLock.lockFile = ./Cargo.lock;
        #   meta = with pkgs.lib; {
        #     inherit (package) description homepage repository;
        #     license = licenses.mit;
        #     maintainers = [ maintainers.xosnrdev ];
        #   };
        # };

        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.cargo-watch
            pkgs.cargo-sort
            pkgs.git-cliff
            pkgs.cargo-release
            pkgs.cargo-dist
            pkgs.git
            pkgs.cargo-bloat
          ];
          shellHook = ''
            export RUST_BACKTRACE=1
          '';
        };

      in {
        formatter = pkgs.nixfmt-classic;
        # packages = { default = snapfind; };
        devShells.default = devShell;
      });
}
