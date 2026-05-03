## Rust → C# トランスパイラ設計まとめ（更新版）

### 目的

Rustコードと **動作が一致する C# コード**を生成する。

優先事項

```
動作一致
Drop順序一致
```

非目標

```
Rust安全性再現
idiomatic C#
最適パフォーマンス
```

対象コードは **自分たちのRustコードのみ**。
unsafeは完全に禁止されている。

---

# コンパイル段階

使用IR

```
HIR of rust analyzer
```

理由

```
structured control flow
async/await情報保持
```

MIRは使用しない。

rust analyzer api

---

# Drop

Dropは **try/finally生成**で実装。

例

```
try {
    body
}
finally {
    drop locals
}
```

---

## partial move

Place tree を作る

```
x
x.a
x.b
```

move発生時

```
place.should_drop = false
```

残りフィールド

```
should_drop = true
```

scope exit

```
reverse creation order
if should_drop
    Drop(place)
```

---

## assign drop

Rust

```
x.a = newValue
```

C#

```
Drop(old x.a)
x.a = newValue
```

---

# temporary lifetime

THIR scope を利用。

temporaryは以下スコープに属する

```
statement
match
if
let reference extension
```

drop順序

```
reverse creation order
```

match guard 等の特殊スコープは **Rustの動作を再現する**。

---

# Ref, fields, and local variables

全ての変数・フィールドは`Slot<T>`にラップされている。

```c#
class Slot<T> : IDisposable {
    public T value;
    public bool dropped; // partial dropするときに外部から操作される
}
```

---

# Box

// TODO: 手動実装の標準ライブラリに含まれる

---

# struct / enum

Rust `struct` ・ `enum` は C# `class` にマップする。

# Enum

enum はパターンマッチングを適用しやすいように

Rust

```
enum E {
    A,
    B(x)
}
```

C#

```
class s_E {
    private s_E() {}
    class v_A {}
    class v_B {
        public Slot<s_x> f_0;
    }
}
```

---

# Result

特別扱いしない。

```
Result<T,E>
```

も通常 enum と同じ変換。

`?` 展開

```
var tmp = expr
if Err(tmp)
    return Err
value = tmp.Ok
```

---

# panic

Rust

```
panic!()
```

↓

C#

```
throw Helpers.Panic(message)
```

Dropは finally で実行。

Drop中のpanicは **多重unwindとして扱う（abortは再現しない）**。

> **Note**: C# では `finally` ブロック内でさらに例外を `throw` すると、元の例外が破棄される。
> Rust の「二重 panic → abort」とは異なり、C# では後発の例外が伝播する。
> この挙動の差異は意図的な非再現事項とする。
> Drop 中に panic が発生した場合、後発の `PanicException` が伝播し、元の例外は失われる。

---

# Trait

Rust

```
trait Foo
```

↓

C#

```
interface t_Foo<TSelf> where TSelf : t_Foo<TSelf>
```

Self

```
TSelf
```

で表現。

> **Note**: `where TSelf : t_Foo<TSelf>` 制約（F-bounded polymorphism）を必ず付ける。
> これにより `t_Foo<ConcreteType>` として使用する際に型安全性が保たれる。

---

# trait object

Rust

```
dyn Trait
```

は empty structs for trait static memers をvtableとして使用する

---

# impl

Rust

```
impl Foo for A
```

↓

C#

```
struct s_A : t_Foo<s_A>
```

（命名規則に従い `s_` prefix が struct、`t_` prefix が trait interface）

---

# 名前衝突

trait間で

```
method
field
```

衝突可能。

対策

```
name mangling
```

例

```
Trait_method
Trait_field
```

---

# Closure

closureは **専用クラス生成**

理由

```
Drop管理
partial capture
move capture
```

Rust

```
let f = || foo.a;
```

C#

```
class Closure {
    Foo foo;

    T Call() {
        return foo.a;
    }
}
```

closure drop時

```
Drop(captured_places)
```

## Closure と Fn/FnMut/FnOnce トレイト

closure は、ラムダ式にする。

---

# Async

Rust

```
async fn
```

↓

C#

```
async RustTask
```

await保持。

Rust側で

```
borrow across await
```

は禁止されているため追加対応不要。

Rustの

```
Future drop = cancel
```

はほぼ再現しない。

```
unused async result
```

に警告を出す。

---

# C# unsafe使用

必要箇所

```
stack ref pointer
Ref<T> access
raw pointer
```

---

# thread_local / static Drop

Rust

```
thread_local
static
```

の Drop は再現しない。

```
intentional leak
```

として扱う。

---

# pointer identity

```
ptr::eq
address comparison
```

は保証しない。

エッジケースとして未対応。

---

# std library

Rust標準ライブラリの多くは **C#で再実装**する。

例

```
Instant → Stopwatch
```

---

# 重要設計

```
THIRベース
独自move/drop解析
Ref<T> = (object?, offset)
stack borrow = absolute pointer (async fn内ローカルはheap扱い)
panic → exception (二重panic時は後発例外が伝播)
enum → class hierarchy (非null, #nullable enable)
trait → interface<TSelf> where TSelf : t_Trait<TSelf>
closure → class (Fn/FnMut/FnOnce interface実装)
struct → struct
raw pointer → Ref<T>
module → mod_ prefix partial class
```

---

# traitの静的メソッド

trait の静的メソッドは別の空のstructに実装させる

これにより JIT の monopolization で呼び出しコストが消えます。

```rust
trait Static {
    fn test() -> ();
}

struct A;

impl Static for A {
    fn test() -> () {
    }
}
```

は

```cs
interface T_Static {
   void test();
}

struct S_A : T_Static {
   void test();
}
```

使用箇所では`default(S_A).test()`のようになる。`S_A`は方引数から

# 命名規則

名前の被りを処理するため以下のように名前を変更する

- modules ⇒ replaced with partial class with `mod_` prefix
- structs ⇒ struct with `s_` prefix
- empty structs for trait static memers ⇒ `S_` prefix (at same module)
- non-dyn trait interfaces ⇒ interface with `t_` prefix
- trait interfaces for static members (including dyn support) => `T_` prefix
- generic parameter represents associated type of trait ⇒ `A_` prefix
- generic parameter represents self of trait => `Self`
- generic parameter represents generic parameter => `P_` prefix
- fields => `f_` prefix
- associated functions => `m_` prefix
- local variables => `l_${name}_index`

> **Note**: modules には `mod_` prefix を使用する（旧設計では `m_` だったが、associated functions の `m_` prefix と衝突するため変更）。

また、すべてのメンバーはpublicとして再実装される。()

# 再現されるRust挙動

```
Drop順序
partial move
borrow semantics
temporary lifetime
async
```

これらをC#で再現する。
