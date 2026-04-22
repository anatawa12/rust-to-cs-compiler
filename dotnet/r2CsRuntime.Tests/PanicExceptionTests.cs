using System;
using r2CsRuntime;

namespace r2CsRuntime.Tests;

public class PanicExceptionTests
{
    [Fact]
    public void PanicException_CanBeThrown()
    {
        Assert.Throws<PanicException>((Action)(() => { throw new PanicException("test panic"); }));
    }

    [Fact]
    public void PanicException_MessageIsPreserved()
    {
        var ex = new PanicException("index out of bounds");
        Assert.Equal("index out of bounds", ex.Message);
    }

    [Fact]
    public void PanicException_InheritsFromException()
    {
        Assert.IsAssignableFrom<Exception>(new PanicException("x"));
    }

    [Fact]
    public void PanicException_CanBeCaughtAsException()
    {
        Exception? caught = null;
        try
        {
            throw new PanicException("caught as base");
        }
        catch (Exception ex)
        {
            caught = ex;
        }
        Assert.NotNull(caught);
        Assert.IsType<PanicException>(caught);
    }

    [Fact]
    public void PanicException_WithInnerException_PreservesInner()
    {
        var inner = new InvalidOperationException("inner");
        var ex = new PanicException("outer", inner);
        Assert.Equal("outer", ex.Message);
        Assert.Same(inner, ex.InnerException);
    }

    /// <summary>
    /// Verifies the try/finally drop-ordering pattern: the Drop runs even when
    /// a PanicException is thrown, matching Rust's unwind-drop behaviour.
    /// </summary>
    [Fact]
    public void PanicException_FinallyRunsDuringUnwind()
    {
        bool dropRan = false;
        try
        {
            Assert.Throws<PanicException>((Action)(() =>
            {
                try
                {
                    throw new PanicException("unwind");
                }
                finally
                {
                    dropRan = true; // simulates Drop(local)
                }
            }));
        }
        catch { /* ignore */ }
        Assert.True(dropRan);
    }
}
