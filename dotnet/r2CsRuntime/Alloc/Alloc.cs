using r2CsRuntime;

namespace VrcGetVpm;

public static partial class crt_Alloc
{
    public static class mod_Collections
    {
        public static class mod_VecDeque
        {
            public class s_VecDeque<T> : IEnumerable<T>
            {
                private T[] _buffer = Array.Empty<T>();
                private int _head;
                private int _count;

                public static s_VecDeque<T> m_New() => new();

                public static s_VecDeque<T> m_FromIter(IEnumerator<T> iterator)
                {
                    var result = m_New();
                    while (iterator.MoveNext())
                        result.m_PushBack(iterator.Current);
                    return result;
                }

                public static s_VecDeque<T> m_FromIter(IEnumerable<T> iterator)
                {
                    var result = m_New();
                    foreach (var item in iterator)
                        result.m_PushBack(item);
                    return result;
                }

                private int m_PhysicalIndex(int index)
                {
                    return (_head + index) % _buffer.Length;
                }

                private void m_EnsureCapacity(int capacity)
                {
                    if (_buffer.Length >= capacity)
                        return;

                    int newCapacity = Math.Max(4, _buffer.Length * 2);
                    if (newCapacity < capacity)
                        newCapacity = capacity;

                    var newBuffer = new T[newCapacity];

                    for (int i = 0; i < _count; i++)
                        newBuffer[i] = _buffer[m_PhysicalIndex(i)];

                    _buffer = newBuffer;
                    _head = 0;
                }

                public crt_Core.mod_Option.s_Option<T> m_PopBack()
                {
                    if (_count == 0)
                        return crt_Core.mod_Option.s_Option<T>.v_None.instance;

                    int index = m_PhysicalIndex(_count - 1);
                    T value = _buffer[index];

                    _buffer[index] = default!;
                    _count--;

                    if (_count == 0)
                        _head = 0;

                    return crt_Core.mod_Option.s_Option<T>.v_Some.ctor(value);
                }

                public crt_Core.mod_Option.s_Option<T> m_PopFront()
                {
                    if (_count == 0)
                        return crt_Core.mod_Option.s_Option<T>.v_None.instance;

                    T value = _buffer[_head];
                    _buffer[_head] = default!;

                    _head = (_head + 1) % _buffer.Length;
                    _count--;

                    if (_count == 0)
                        _head = 0;

                    return crt_Core.mod_Option.s_Option<T>.v_Some.ctor(value);
                }

                public void m_PushFront(T value)
                {
                    m_EnsureCapacity(_count + 1);

                    _head = (_head - 1 + _buffer.Length) % _buffer.Length;
                    _buffer[_head] = value;
                    _count++;
                }

                public void m_PushBack(T value)
                {
                    m_EnsureCapacity(_count + 1);

                    _buffer[m_PhysicalIndex(_count)] = value;
                    _count++;
                }

                public IEnumerator<T> m_Iter()
                {
                    for (int i = 0; i < _count; i++)
                        yield return _buffer[m_PhysicalIndex(i)];
                }

                public IEnumerator<T> m_Retain(Func<T, bool> predicate)
                {
                    int write = 0;

                    for (int read = 0; read < _count; read++)
                    {
                        T value = _buffer[m_PhysicalIndex(read)];

                        if (predicate(value))
                        {
                            _buffer[m_PhysicalIndex(write)] = value;
                            write++;
                        }
                    }

                    for (int i = write; i < _count; i++)
                        _buffer[m_PhysicalIndex(i)] = default!;

                    _count = write;

                    if (_count == 0)
                        _head = 0;

                    return m_Iter();
                }

                public IEnumerator<T> GetEnumerator() => m_Iter();

                System.Collections.IEnumerator System.Collections.IEnumerable.GetEnumerator() => GetEnumerator();
            }
        }
    }
}