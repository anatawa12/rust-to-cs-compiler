namespace r2CsRuntime;

/// <summary>
/// Corresponds to Rust's <c>FnOnce</c> trait.
/// The call conceptually consumes <c>self</c>; the compiler guarantees the
/// object is not used again after <c>m_call_once</c> is invoked.
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_FnOnce<TSelf, TArgs, TReturn>
    where TSelf : t_FnOnce<TSelf, TArgs, TReturn>
{
    TReturn m_call_once(TArgs args);
}

/// <summary>
/// Corresponds to Rust's <c>FnMut</c> trait.
/// The call may mutate captured variables.
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_FnMut<TSelf, TArgs, TReturn> : t_FnOnce<TSelf, TArgs, TReturn>
    where TSelf : t_FnMut<TSelf, TArgs, TReturn>
{
    TReturn m_call_mut(TArgs args);
}

/// <summary>
/// Corresponds to Rust's <c>Fn</c> trait.
/// The call only reads captured variables; the closure may be called
/// concurrently from multiple callers.
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_Fn<TSelf, TArgs, TReturn> : t_FnMut<TSelf, TArgs, TReturn>
    where TSelf : t_Fn<TSelf, TArgs, TReturn>
{
    TReturn m_call(TArgs args);
}

// ── Dyn-capable interfaces (T_ prefix) ──────────────────────────────────────
//
// These correspond to the "trait interfaces for static members (including dyn
// support)" described in the naming-convention design.  Each T_Fn* method
// receives the full fat pointer (DynRef<T_Interface>) so the caller never
// needs to know the concrete type.  The S_ vtable struct's implementation
// casts the data portion to the concrete type with AsConcreteRef<T>.
//
// Call pattern (dyn FnMut(int)->int):
//
//   struct s_MyClosure : t_FnMut<s_MyClosure,int,int> { int f_counter; ... }
//
//   struct S_MyClosure : T_FnMut<int,int> {
//       int T_FnOnce<int,int>.m_call_once(DynRef<T_FnOnce<int,int>> self, int args) {
//           // upcast vtable to T_FnMut to reuse m_call_mut
//           var asMut = new DynRef<T_FnMut<int,int>>(
//               self.Target!, self.Offset, (T_FnMut<int,int>)self.Vtable);
//           return ((T_FnMut<int,int>)self.Vtable).m_call_mut(asMut, args);
//       }
//       int T_FnMut<int,int>.m_call_mut(DynRef<T_FnMut<int,int>> self, int args) {
//           ref s_MyClosure c = ref self.AsConcreteRef<s_MyClosure>();
//           c.f_counter += args;
//           return c.f_counter;
//       }
//   }
//
//   // calling through DynRef:
//   DynRef<T_FnMut<int,int>> dynRef = new(obj, offset, new S_MyClosure());
//   int result = dynRef.Vtable.m_call_mut(dynRef, args);

/// <summary>
/// Dyn-dispatch interface for Rust's <c>FnOnce</c>.
/// Implemented by <c>S_</c>-prefixed vtable structs.
/// The self parameter is the full fat pointer so callers need not know the concrete type.
/// </summary>
public interface T_FnOnce<TArgs, TReturn>
{
    TReturn m_call_once(DynRef<T_FnOnce<TArgs, TReturn>> self, TArgs args);
}

/// <summary>
/// Dyn-dispatch interface for Rust's <c>FnMut</c>.
/// Implemented by <c>S_</c>-prefixed vtable structs.
/// </summary>
public interface T_FnMut<TArgs, TReturn> : T_FnOnce<TArgs, TReturn>
{
    TReturn m_call_mut(DynRef<T_FnMut<TArgs, TReturn>> self, TArgs args);
}

/// <summary>
/// Dyn-dispatch interface for Rust's <c>Fn</c>.
/// Implemented by <c>S_</c>-prefixed vtable structs.
/// </summary>
public interface T_Fn<TArgs, TReturn> : T_FnMut<TArgs, TReturn>
{
    TReturn m_call(DynRef<T_Fn<TArgs, TReturn>> self, TArgs args);
}
