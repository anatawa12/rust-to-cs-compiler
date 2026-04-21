# rust to cs compiler

This is experiment to transpile rust code to C# to transpile [vrc-get-vpm]
to C# for [VPMPackageAutoInstaller].

## Basic design

- Requires async support, with compiling to `async` methods in C#
  - No cancellation support since Task cannot support them
- Export partially readable code
  - For this design, we use HIR with THIR for generating code
- Drop support as possible
  - As noted above, cancellation of future is exception to this

Detailed Design goes in `desgin` directory

[vrc-get]: https://github.com/vrc-get/vrc-get/tree/master/vrc-get-vpm
[VPMPackageAutoInstaller]: https://github.com/anatawa12/VPMPackageAutoInstaller
