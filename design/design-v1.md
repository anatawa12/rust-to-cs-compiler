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

---

# コンパイル段階

使用IR

```
MIR (Mid-level Intermediate Representation)
```

理由

```
rustc の after_analysis フェーズで利用可能
THIR は analysis フェーズで stolen されるため使用不可
optimized_mir() で最適化済みMIRを取得
```

> **Note**: 当初 THIR を使用する設計だったが、`thir_body()` は borrow checker が実行される
> `analysis` フェーズ中に consume されるため、`after_analysis` callback からは利用できない。
> MIR はこの問題がなく、`optimized_mir()` で安定して取得できる。

MIRは使用しない。← **廃止済み記述**

---

# 必要解析

MIR を直接使用するため、独自の move/borrow 解析は **不要**。
rustc の MIR にはすでに以下の情報が含まれている：

```
DropKind / drop flags
Operand::Move / Operand::Copy
PlaceRef / projections
Closure upvars
```

---

# Drop

MIR の `TerminatorKind::Drop` を直接 C# に変換する。

変換規則:

```
Drop { place, target } →
  if tcx.adt_destructor(ty).is_some():
    T.m_drop(&place);
  goto target;
```

`Drop` trait の実装は `static unsafe Void m_drop(T* _1)` として生成される。
外部トレイト（Drop, Clone など）はインターフェース生成をスキップする。

> **Note**: 旧設計の try/finally による Drop は **未実装**。
> MIR にすでに Drop の順序が埋め込まれているため、
> try/finally を生成する代わりに unwind パスのブロックを直接出力する。

---

## partial move

MIR の `Operand::Move` と `PlaceRef` projection で表現済み。
独自の PlaceTree 解析は不要。

---

## assign drop

MIR では assign の前に必要な Drop が別のターミネーターとして挿入済み。

---

# temporary lifetime

MIR のブロック境界に落とし込まれているため独自解析不要。

---

# Ref / 参照表現

**実装済みの表現（現在）:**

```
T* (C# raw pointer)
```

`&T`, `&mut T`, `*const T`, `*mut T`（非 fat）はすべて `T*` にマップする。

生成されるすべての関数は `unsafe` でマークされる。

> **Note**: 旧設計では `Ref<T> = (object? target, nint offset)` を使用する予定だったが、
> MIR ベースではスタックの局所変数は C# のスタックローカルとして直接生成されるため、
> `T*` で取得可能。`Var<T>` ボクシング機構も不要となった。

---

# Unsized Ref

## slices

```
LenRef<Slice<T>>  ←  &[T], *const [T] など fat pointer
```

## dyn Trait

```
DynRef<T>  ←  &dyn Trait （未実装、TODO）
```

---

## stack borrow

MIR ベースでは、スタックローカルは C# ローカル変数として生成される。
`&local` → `&local`（C# の `fixed` 不要）。`unsafe` コンテキスト内で有効。

`Var<T>` クラスによるボクシングは**廃止**。

---

# Raw pointer

```
*const T
*mut T
```

→ `T*`（C# raw pointer）

旧設計の `Pointer<T>` / `LenPointer<T>` / `DynPointer<T>` は**廃止**。

---

# Box

// TODO: 手動実装の標準ライブラリに含まれる

---

# struct / enum

Rust `struct` ・ `enum` は C# `struct` にマップする。

> **Note**: 旧設計では closure environment と async state machine はヒープオブジェクトとしていたが、
> 現在の MIR ベース実装では closure は static method として生成される（後述）。

---

# Enum

Rust

```
enum E {
    A,
    B(x)
}
```

C#

```
struct s_E {
    public byte f_discriminant;
    // variant payload structs (nested):
    public s_E_B f_B;

    public struct s_E_B {
        public x f_0;
    }
}
```

タグは `byte` 型の `f_discriminant` フィールド。
各バリアントのペイロードは内部ネストした struct として表現。

---

# Result

特別扱いしない。通常 enum と同じ変換。

---

# panic

Rust

```
panic!()
```

↓

C#

```
throw new global::r2CsRuntime.PanicException("message");
```

MIR の unwind パスは `throw new PanicException("unwind")` を生成。

---

# Trait

Rust

```
trait Foo {
    fn bar(self) -> T;
}
```

↓

C#

```
public interface t_Foo<Self> where Self : t_Foo<Self> {
    static abstract T m_bar(Self _1);
}
```

Self は `Self`（旧設計では `TSelf` だったが現実装では `Self`）で表現。

**外部クレートのトレイト**（`core::ops::Drop` など）は C# インターフェースを生成しない。

---

# trait object

Rust

```
dyn Trait
```

→ TODO（未実装）

---

# impl

Rust

```
impl Foo for A
```

↓

C#

```
public partial struct s_A : t_Foo<s_A> { ... }
```

ローカルトレイトのみインターフェース句を追加。外部トレイト（Drop など）はスキップ。

---

# 名前衝突

trait 間でメソッド・フィールド名が衝突する場合は name mangling で対処（TODO）。

---

# Closure

**現在の実装**：

クロージャは static method の **shim + impl ペア**として生成される。

```csharp
// shim: FnOnce ABI (env_ptr + tuple-packed args)
public static unsafe int m_apply_closure_0(Void* _1, ValueTuple<int> _2)
    => m_apply_closure_0_impl(_1, _2.Item1);

// impl: actual MIR body (individual args)
public static unsafe int m_apply_closure_0_impl(Void* _1 /* env */, int _2 /* v */) { ... }
```

名前は `DefPathData::Closure` → `_closure_N` サフィックスで mangle される。

キャプチャなしクロージャの環境型は `Void`（ZST）。
キャプチャありクロージャの完全サポートは TODO。

> **Note**: 旧設計ではクロージャを専用クラスとして生成し、
> `Fn`/`FnMut`/`FnOnce` インターフェースを実装する予定だったが、
> MIR ベースでは shim/impl の static method パターンを採用した。

---

# Async

未実装 (TODO)。

> **Note**: C# async/await と Rust async は根本的に非互換。
> - Rust の Future は stack-pinned struct（ヒープ不要）
> - C# の async Task は GC ヒープ上のクラスオブジェクト
>
> **採用方針**: Rust の async fn は C# の `unsafe` な手動ステートマシン struct として生成する。
> `poll()` に相当するメソッドを持ち、再開ポイントは int フィールドで管理する。
> これにより C# のヒープを使わず Rust の非ヒープセマンティクスを再現できる。
>
> `async Task` は使用しない。

---

# C# unsafe 使用

生成されるすべての関数は `unsafe` でマークされる。

```
T* 参照操作
fixed スタックポインタ
raw pointer 演算
```

---

# mem::zeroed

```
mem::zeroed<T>() → default(T)
```

C# struct に変換される型のみ有効。

---

# thread_local / static Drop

再現しない（intentional leak）。

---

# pointer identity

保証しない（未対応エッジケース）。

---

# std library

Rust 標準ライブラリの多くは C# で再実装する。
`StdLib.cs` にスタブを配置。

例:
- `str::as_bytes` → UTF-8 バイト列を返すスタブ
- `<[T]>::is_empty` → `slice.Length == 0`
- `SliceExt.GetElement` → 範囲チェック付き要素取得

---

# 重要設計（現在の実装）

```
MIRベース（THIRは使用しない）
独自move/drop解析不要（MIRに含まれる）
T* raw pointer（Ref<T>/Var<T>/Pointer<T> は廃止）
&[T] → LenRef<Slice<T>>（fat pointer）
panic → PanicException（throw）
enum → tagged union struct（f_discriminant + nested payload structs）
trait → interface t_Foo<Self> where Self : t_Foo<Self>（ローカルトレイトのみ）
closure → static shim+impl pair（_closure_N suffix）
struct → struct
module → mod_ prefix partial class
すべての生成関数は unsafe
```

---

# traitの静的メソッド

trait の静的メソッドは `static abstract` として interface に定義する。

```rust
trait Foo {
    fn test() -> ();
}
impl Foo for A { fn test() {} }
```

```csharp
public interface t_Foo<Self> where Self : t_Foo<Self> {
    static abstract void m_test();
}
public partial struct s_A : t_Foo<s_A> {
    public static unsafe void m_test() { }
}
```

呼び出し側では型パラメータ経由で `T.m_test()` のように呼ぶ。

---

# 命名規則

名前の被りを処理するため以下のように名前を変更する

- modules ⇒ replaced with partial class with `mod_` prefix
- structs ⇒ struct with `s_` prefix
- enums ⇒ struct with `s_` prefix（enum と struct は同じルール）
- empty structs for trait static members ⇒ `S_` prefix（予約、未実装）
- non-dyn trait interfaces ⇒ interface with `t_` prefix
- trait interfaces for static members (including dyn support) ⇒ `T_` prefix（予約、未実装）
- generic parameter represents associated type of trait ⇒ `A_` prefix（予約、未実装）
- generic parameter represents self of trait ⇒ `Self`
- generic parameter represents generic parameter ⇒ `P_` prefix
- fields ⇒ `f_` prefix
- associated functions ⇒ `m_` prefix
- local variables ⇒ `_N`（MIR の local index をそのまま使用）

> **Note**: ローカル変数は旧設計の `l_${name}_index` ではなく MIR の `_N`（`_0`, `_1`, ...）を使用。
> デバッグ用の元の変数名はコメントで補足する（例: `int _1 /* x */`）。

また、すべてのメンバーは `public` として生成される。

---

# 再現されるRust挙動

```
Drop順序（MIR の Drop ターミネーターで表現）
partial move（MIR の Operand::Move で表現）
panic → PanicException
```

未実装:
```
async / Future
capturing closures
dyn Trait
trait objects
```


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

---

# コンパイル段階

使用IR

```
THIR
```

理由

```
structured control flow
async/await情報保持
```

MIRは使用しない。

---

# 必要解析

独自解析で以下を実装

```
MovePath解析
DropFlag
Temporary lifetime
Closure capture
Stack ref escape
Borrow escape
Struct borrow detection
```

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

# Ref

共通表現

```
Ref<T> = (object? target, nint offset)
```

意味

| case          | target     | offset           |
|---------------| ---------- | ---------------- |
| on-heap field | object     | field offset     |
| stack         | null       | absolute pointer |

# Unsized Ref

## slices

sliceが最後のフィールドであるstructも同様

```
LenRef<T> = (object? target, nint offset, nint len)
```

## dyn Trait

dyn traitが最後のフィールドであるstructも同様

```
DynRef<T> = (object? target, nint offset, T vtable)
```

Tは`trait interfaces for static members`。
実際に作る際には defualt(structs for trait static memers)を渡す。

trait interfaces for static membersのdyn部分はRef<Void>で関数内でRef<s_Struct>に強制キャストする

---

## stack borrow

条件

```
awaitを跨がない
closure captureされない
return escapeしない
```

成立する場合

```
&local → pointer
```

実装

```
target = null
offset = stack address
```

それ以外

```
class Var<T>
```

> **Note**: `async fn` 内のローカル変数は、コンパイラが生成する async state machine（ヒープオブジェクト）に
> 格納されるため、実態はスタック上に存在しない。
> stack borrow 解析では `async fn` のローカルを「スタック変数ではない」として扱い、
> `await` を跨ぐ判定とは独立して stack borrow に分類しないこと。
> これらの変数への参照は heap borrow として `Var<T>` 経由で扱う。

---

# Raw pointer

```
*const T
*mut T
```

内部表現

```
Pointer<T>
LenPointer<T>
DynPointer<T>
```

中身はRef<T>と同様

制限

```
strict provenance
```

禁止

```
expose_addr
with_addr
```

---

# Box

// TODO: 手動実装の標準ライブラリに含まれる

---

# struct / enum

Rust `struct` ・ `enum` は C# `struct` にマップする。

ただし以下の型は **ヒープオブジェクトとして実装される**

```
closure environment
async state machine
```

---

# Enum

Rust

```
enum E {
    A,
    B(x)
}
```

C#

```
struct s_E {
    public int tag;
    public x f_0;
}
```

(f_0はAのときに使われない)

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
throw PanicException
```

```
class PanicException : Exception
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

closure クラスは、対応する `Fn` / `FnMut` / `FnOnce` の trait interface を実装する。

| Rust trait | C# interface        | 制約                                      |
|------------|---------------------|-------------------------------------------|
| `FnOnce`   | `t_FnOnce<Args, R>` | `Call(Args) -> R`（一度だけ呼び出し可能） |
| `FnMut`    | `t_FnMut<Args, R>`  | `CallMut(ref self, Args) -> R`            |
| `Fn`       | `t_Fn<Args, R>`     | `CallRef(in self, Args) -> R`             |

`Fn : FnMut : FnOnce` の継承関係を C# interface 継承で表現する。
具体的には `t_Fn<Args,R> : t_FnMut<Args,R> : t_FnOnce<Args,R>`。

---

# Async

Rust

```
async fn
```

↓

C#

```
async Task
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

は再現しない。

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

# mem::zeroed

以下の型のみ許可

```
C# struct に変換される型
```

禁止

```
closure environment
async state machine
trait object
Box
```

変換

```
mem::zeroed<T>() → default(T)
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

