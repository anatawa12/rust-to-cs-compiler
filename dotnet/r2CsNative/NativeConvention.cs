// r2CsNative — hand-written C# implementations for types/methods marked #[r2cs_native].
//
// CONVENTION
// ----------
// When a Rust struct or enum is annotated with #[cfg_attr(r2cs, r2cs_native)], the transpiler
// emits only a comment placeholder instead of a generated class.  Add a complete class here.
//
// When a Rust method is annotated with #[cfg_attr(r2cs, r2cs_native)], the transpiler skips that
// method in the generated output (the generated class is always `partial`).  Add the method here
// as a partial-class extension so the C# compiler merges the two halves.
//
// EXAMPLE — native type (entire class hand-written)
// --------------------------------------------------
// Rust (in vrc-get-vpm):
//   #[cfg_attr(r2cs, r2cs_native)]
//   pub struct MyHandle { ... }
//
// C# (this project, e.g. MyHandle.cs):
//   namespace VrcGetVpm;
//   public partial class s_MyHandle {
//       // full hand-written implementation
//   }
//
// EXAMPLE — native method (partial extension)
// -------------------------------------------
// Rust:
//   impl SomeType {
//       #[cfg_attr(r2cs, r2cs_native)]
//       pub fn expensive_parse(input: &str) -> Option<Self> { ... }
//   }
//
// C# (SomeType.Native.cs):
//   namespace VrcGetVpm;
//   public partial class s_SomeType {
//       public static s_Option<s_SomeType> m_ExpensiveParse(string input) {
//           // hand-written implementation
//       }
//   }
//
// NOTE: The namespace must match the one emitted by the transpiler (PascalCase crate name).

namespace r2CsNative;

// Placeholder so the project compiles even when no native overrides exist yet.
internal static class _Placeholder { }
