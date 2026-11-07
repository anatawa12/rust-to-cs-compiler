using System.Collections;
using System.Reflection;
using r2CsRuntime;

namespace VrcGetVpm;

public static partial class crt_Alloc
{
    public static partial class mod_Vec
    {
        public class s_Vec<T> : IEnumerable<T>
        {
            public struct Statics
            {
            }

            private T[] body;
            private int length;

            public s_Vec(T[] contents)
            {
                body = contents;
                length = contents.Length;
            }
            private s_Vec(T[] contents, int length)
            {
                body = contents;
                this.length = length;
            }

            public static s_Vec<T> m_New() => new s_Vec<T>(Array.Empty<T>());
            public static s_Vec<T> m_Default() => m_New();
            public static s_Vec<T> m_WithCapacity(nuint len) => new(new T[len], 0);
            
            public static s_Vec<T> m_FromIter(IEnumerator<T> iterator)
            {
                var result = m_New();
                result.m_Extend(iterator);
                return result;
            }

            public nuint m_Len() => (nuint)length;
            public bool m_IsEmpty() => length == 0;
            //public static implicit operator Slice<T>(s_Vec<T> self) => self.m_AsSlice();
            public Slice<T> m_Deref() => m_AsSlice();

            public IEnumerator<T> m_Iter() => body.Take(length).GetEnumerator();
            public IEnumerator<T> m_IntoIter() => m_Iter();
            IEnumerator<T> IEnumerable<T>.GetEnumerator() => m_Iter();
            IEnumerator IEnumerable.GetEnumerator() => m_Iter();
            public Slice<T> m_AsSlice() => Slice<T>.Of(body, 0, length);
            
            public void m_Push(T value)
            {
                m_Reserve(1);
                body[length++] = value;
            }

            public void m_Reserve(nuint count)
            {
                if ((nuint)length + count > int.MaxValue) throw new PanicException("Size Exceeds");
                if (length + (int)count > body.Length)
                {
                    var newLen = Math.Max(length + (int)count, body.Length == 0 ? 16 : body.Length * 2);
                    var newBuf = new T[newLen];
                    body.CopyTo(newBuf, 0);
                    body = newBuf;
                }
            }
            
            public void m_Retain(Func<T, bool> cond)
            {
                int srcI = 0, dstI = 0, length = this.length;
                this.length = 0;
                try
                {
                    while (srcI < length)
                    {
                        if (cond(body[srcI]))
                        {
                            body[dstI] = body[srcI];
                            dstI++;
                        }

                        srcI++;
                    }
                }
                finally
                {
                    // cond panics (throws). copy body[srcI..length] => body[dstI..]
                    // This part will be no-op when no panic happened (srcI == length)
                    body.AsSpan(srcI, length - srcI).CopyTo(body.AsSpan(dstI));
                    dstI += length - srcI;

                    this.length = dstI;
                }
            }
            
            public T m_Remove(nuint index)
            {
                if (index >= m_Len()) throw new PanicException("Index out of range");
                var result = body[index];
                body.AsSpan((int)index + 1).CopyTo(body.AsSpan((int)index));
                length -= 1;
                return result;
            }

            public void m_Extend(IEnumerable<T> iterator) => m_Extend(iterator.GetEnumerator());
            public void m_Extend(IEnumerator<T> iterator)
            {
                using var iteratorInner = iterator;
                while (iteratorInner.MoveNext())
                    m_Push(iteratorInner.Current);
            }

            public void ExtendFromSpan(ReadOnlySpan<T> data)
            {
                m_Reserve((nuint)data.Length);
                data.CopyTo(body.AsSpan(length));
                length += data.Length;
            }

            public IEnumerator<T> m_ExtractIf(Range<nuint> range, Func<T, bool> pred) => new ExtractIf(this, range, pred);

            class ExtractIf : IEnumerator<T>
            {
                private s_Vec<T> _vec;
                private int _idx;
                private int _end;
                private int _del;
                private int _old_len;
                private Func<T, bool> _pred;

                public ExtractIf(s_Vec<T> vec, Range<nuint> range, Func<T, bool> pred)
                {
                    _vec = vec;
                    _del = 0;
                    var (idx, end) = range.GetIndexRage(vec.m_Len());
                    _idx = (int)idx;
                    _end = (int)end;
                    _old_len = vec.length;
                    _pred = pred;
                    Current = default!;
                }
                
                public T Current { get; private set; }
                object? IEnumerator.Current => Current;

                public bool MoveNext()
                {
                    while (_idx < _end) {
                        var i = _idx;
                        var cur = _vec.body[i];
                        var drained = _pred(cur);
                        _idx += 1;
                        if (drained) {
                            _del += 1;
                            Current = cur;
                            return true;
                        } else if (_del > 0) {
                            _vec.body[i - _del] = cur;
                        }
                    }

                    return false;
                }

                public void Reset() => throw new InvalidOperationException();

                public void Dispose()
                {
                    if (_del > 0) {
                        _vec.body.AsSpan(_idx, _old_len - _idx)
                            .CopyTo(_vec.body.AsSpan(_idx - _del));
                    }
                    _vec.length = _old_len - _del;
                }
            }

            public void m_Clear()
            {
                length = 0;
            }

            // slice functions
            public Slice<T> Index(Range<nuint> range) => m_AsSlice().Index(range);
        }
    }
}
