/// Tests for the generated C# code from tests/inputs/strings.rs.
public class StringTests
{
    // ── first_char_code ───────────────────────────────────────────────────

    [Fact]
    public void First_char_code_hello() =>
        Assert.Equal((byte)'H', mod_strings.m_first_char_code("Hello"));

    [Fact]
    public void First_char_code_empty() =>
        Assert.Equal(0, mod_strings.m_first_char_code(""));

    [Fact]
    public void First_char_code_a() =>
        Assert.Equal((byte)'a', mod_strings.m_first_char_code("abc"));

    // ── is_ascii_digit ────────────────────────────────────────────────────

    [Fact]
    public void Is_ascii_digit_five() =>
        Assert.True(mod_strings.m_is_ascii_digit('5'));

    [Fact]
    public void Is_ascii_digit_zero() =>
        Assert.True(mod_strings.m_is_ascii_digit('0'));

    [Fact]
    public void Is_ascii_digit_nine() =>
        Assert.True(mod_strings.m_is_ascii_digit('9'));

    [Fact]
    public void Is_ascii_digit_letter() =>
        Assert.False(mod_strings.m_is_ascii_digit('a'));

    [Fact]
    public void Is_ascii_digit_space() =>
        Assert.False(mod_strings.m_is_ascii_digit(' '));

    // ── is_ascii_alpha ────────────────────────────────────────────────────

    [Fact]
    public void Is_ascii_alpha_lowercase() =>
        Assert.True(mod_strings.m_is_ascii_alpha('z'));

    [Fact]
    public void Is_ascii_alpha_uppercase() =>
        Assert.True(mod_strings.m_is_ascii_alpha('A'));

    [Fact]
    public void Is_ascii_alpha_digit() =>
        Assert.False(mod_strings.m_is_ascii_alpha('3'));

    // ── ascii_to_upper ────────────────────────────────────────────────────

    [Fact]
    public void Ascii_to_upper_a() =>
        Assert.Equal('A', mod_strings.m_ascii_to_upper('a'));

    [Fact]
    public void Ascii_to_upper_z() =>
        Assert.Equal('Z', mod_strings.m_ascii_to_upper('z'));

    [Fact]
    public void Ascii_to_upper_already_upper() =>
        Assert.Equal('B', mod_strings.m_ascii_to_upper('B'));

    [Fact]
    public void Ascii_to_upper_digit_unchanged() =>
        Assert.Equal('7', mod_strings.m_ascii_to_upper('7'));

    // ── ascii_to_lower ────────────────────────────────────────────────────

    [Fact]
    public void Ascii_to_lower_A() =>
        Assert.Equal('a', mod_strings.m_ascii_to_lower('A'));

    [Fact]
    public void Ascii_to_lower_Z() =>
        Assert.Equal('z', mod_strings.m_ascii_to_lower('Z'));

    [Fact]
    public void Ascii_to_lower_already_lower() =>
        Assert.Equal('b', mod_strings.m_ascii_to_lower('b'));

    // ── char_digit_value ──────────────────────────────────────────────────

    [Fact]
    public void Char_digit_value_zero() =>
        Assert.Equal(0, mod_strings.m_char_digit_value('0'));

    [Fact]
    public void Char_digit_value_nine() =>
        Assert.Equal(9, mod_strings.m_char_digit_value('9'));

    [Fact]
    public void Char_digit_value_five() =>
        Assert.Equal(5, mod_strings.m_char_digit_value('5'));

    [Fact]
    public void Char_digit_value_letter() =>
        Assert.Equal(-1, mod_strings.m_char_digit_value('x'));
}
