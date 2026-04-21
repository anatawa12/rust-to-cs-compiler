/// Tests for the generated C# code from tests/inputs/borrowing.rs.
public class BorrowingTests
{
    [Fact]
    public void Sum_refs() => Assert.Equal(6, mod_borrowing.m_sum_refs(1, 2, 3));

    [Fact]
    public void Sum_refs_zero() => Assert.Equal(0, mod_borrowing.m_sum_refs(0, 0, 0));

    [Fact]
    public void Max_by_ref_first() => Assert.Equal(5, mod_borrowing.m_max_by_ref(5, 3));

    [Fact]
    public void Max_by_ref_second() => Assert.Equal(7, mod_borrowing.m_max_by_ref(3, 7));

    [Fact]
    public void Max_by_ref_equal() => Assert.Equal(4, mod_borrowing.m_max_by_ref(4, 4));

    [Fact]
    public void Double_local() => Assert.Equal(10, mod_borrowing.m_double_local(5));

    [Fact]
    public void Double_local_zero() => Assert.Equal(0, mod_borrowing.m_double_local(0));

    [Fact]
    public void Increment_local() => Assert.Equal(6, mod_borrowing.m_increment_local(5));
}
