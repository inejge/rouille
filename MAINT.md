## Maintenance release of the rouille library

The last published release of rouille was 3.0.0 (07-Dec-2018), and the last
commit in the GitHub repository was made on 06-Aug-2019. Rust's stability
commitments mean that the library is still usable with modern compilers, but
outdated dependencies result in code bloat and old constructs such as `try!()`
produce warnings which can't be silenced selectively. This maintenance release
aims to mitigate these problems without otherwise changing the library.

### Disclaimer

__This crate is not the work of rouille's original author, and is not endorsed
or supported by him. Use at your own risk, or vendor the original and selectively
apply the patches.__

### Using this crate as a drop-in replacement for rouille

In order to be published on crates.io, a crate must have a unique name. This crate
is named `rouille-maint-in`: _maint_ for maintenance, _in_ for the author's initials.
To use it in place of rouille 3.0.0, put the following in your `Cargo.toml`:

```
rouille = { version = "3", package = "rouille-maint-in" }
```

If you're using any of rouille's package features, specify them in the same way.

### Further development

In my (@inejge) opinion, rouille is perfectly fine as it is, and doesn't need more
features. Bug fixes and dependency upgrades, if needed, are welcome.
