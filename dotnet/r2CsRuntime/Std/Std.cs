using System.Collections;
using r2CsRuntime;

namespace VrcGetVpm;

public partial class crt_Std
{
    public static partial class mod_Ffi
    {
        public static partial class mod_OsStr
        {
            public partial class s_OsStr
            {
                public static partial crt_Std.mod_Ffi.mod_OsStr.s_OsStr m_FromEncodedBytesUnchecked(Slice<byte> l_bytes_0) =>
                    s_OsStr.ctor(l_bytes_0);

                public bool m_StartsWith(RustStr l_suffix_0) => m_StartsWith(s_OsStr.m_New(l_suffix_0));
                public bool m_EndsWith(RustStr l_suffix_0) => m_EndsWith(s_OsStr.m_New(l_suffix_0));
            }

            public partial class s_OsString
            {
                public void m_Push(RustStr l_other_0) => m_Push(s_OsStr.m_New(l_other_0));
                public void m_Push(s_OsString l_other_0) => m_Push(l_other_0.m_Deref());

                public static crt_Std.mod_Ffi.mod_OsStr.s_OsString m_From(mod_Path.s_Path l_value_0) =>
                    throw new NotImplementedException(); // No Path implementation ATM
            }
        }
    }

    public static partial class mod_Path
    {
        public partial class s_Path
        {
            public static partial crt_Std.mod_Path.s_Path m_New(crt_Std.mod_Ffi.mod_OsStr.s_OsStr l_s_0) => new s_Path { f_inner = l_s_0 };
            public static s_Path m_New(RustStr packages) => m_New(crt_Std.mod_Ffi.mod_OsStr.s_OsStr.m_New(packages));
            public s_PathBuf m_Join(mod_Ffi.mod_OsStr.s_OsString component) => m_Join(m_New(component.m_Deref()));
            public s_PathBuf m_Join(mod_Ffi.mod_OsStr.s_OsStr component) => m_Join(m_New(component));
            public s_PathBuf m_WithExtension(RustStr zipSha256) => throw null;
            public s_Path m_AsRef() => this;
            public static s_Path m_From(s_PathBuf path) => m_New(path.m_IntoOsString().m_Deref());
            public s_Display m_Display() => new s_Display(this);
           
            public static implicit operator s_Path(RustStr original) => m_New(original);
            public static implicit operator s_Path(mod_Ffi.mod_OsStr.s_OsStr original) => m_New(original);
            public static implicit operator s_Path(crt_Alloc.mod_String.s_String original) => original.m_AsStr();
        }
        
        public struct s_Display : crt_Core.mod_Fmt.t_Display
        {
            private s_Path _inner;

            public s_Display(s_Path inner) => _inner = inner;

            public crt_Alloc.mod_String.s_String m_ToString() => _inner.f_inner.m_ToStringLossy().m_ToOwned();

            public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DisplayFmt(crt_Core.mod_Fmt.s_Formatter p_0) =>
                p_0.m_WriteStr(_inner.f_inner.m_ToStringLossy());
        }

        public partial class s_PathBuf
        {
            public static implicit operator s_Path(s_PathBuf buf) => buf.m_Deref();
            public void m_Push(mod_Ffi.mod_OsStr.s_OsStr component) => m_Push(s_Path.m_New(component));
            public void m_Push(RustStr component) => m_Push(mod_Ffi.mod_OsStr.s_OsStr.m_New(component));
            public void m_SetExtension(RustStr extension) => m_SetExtension(mod_Ffi.mod_OsStr.s_OsStr.m_New(extension));
            public s_PathBuf m_Joined(mod_Ffi.mod_OsStr.s_OsString path) => m_Joined(s_Path.m_New(path.m_AsRef()));
            public s_PathBuf m_Joined(RustStr path) => m_Joined(s_Path.m_New(path));
            public s_Path m_IntoBoxedPath() => this.m_Deref();
        }

        public partial class s_Components
        {
            public void Dispose()
            {
            }

            public s_Component Current => f_current;

            public void Reset() => throw new NotSupportedException();
            object? IEnumerator.Current => Current;
        }
    }
}