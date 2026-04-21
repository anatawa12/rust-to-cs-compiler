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

---

# 実装メモ・設計上の問題点 (Implementation Notes & Design Issues)

以下は初期実装（`dotnet/r2CsRuntime/`・`src/`）の過程で発見された設計上の問題点や注意事項。

## C# ランタイム実装 (`dotnet/r2CsRuntime/`)

### 実装済みファイル

| ファイル | 内容 |
|---|---|
| `PanicException.cs` | `panic!()` に対応する例外クラス |
| `Var<T>.cs` | スタックエスケープするローカル変数のヒープボックス |
| `Drop.cs` | `t_Drop<TSelf>` trait インターフェース |
| `Fn.cs` | `t_FnOnce<TSelf,TArgs,TReturn>` / `t_FnMut` / `t_Fn` trait インターフェース |
| `Void.cs` | 零サイズ unit 型（dyn vtable erasure 用） |
| `Internal/RawData.cs` | CLR オブジェクトレイアウトの内部ヘルパー |
| `Ref.cs` | `Ref<T>` + `RefHelper` |
| `LenRef.cs` | `LenRef<T>` — スライス参照 |
| `DynRef.cs` | `DynRef<TVtable>` — dyn trait 参照 |
| `Pointer.cs` | `Pointer<T>`, `LenPointer<T>`, `DynPointer<T>` — 生ポインタ |

### 問題点1: C# 10 では `FnMut`/`Fn` の `ref self` / `in self` を interface method で表現できない

Rust の `FnMut::call_mut(&mut self, ...)` に対応する C# インターフェースメソッドは、
C# 10 (netstandard2.0) では `ref TSelf` パラメータを持つ static abstract メソッドとして表現できない。
**C# 11 の `static abstract interface members`** が必要。

現在の回避策: instance method として実装。struct 実装者の場合、インターフェース経由の呼び出しは boxing が必要になるが、
コンパイラが常に具体型で直接呼び出すため、実際には boxing は発生しない。

### 問題点2: vtable struct は C# では真の零サイズにならない

設計では vtable struct (`S_Foo_for_s_Bar`) が "empty struct = zero-sized" であることを前提としているが、
C# の struct は最小サイズが 1 バイト（CLR の制約）。

**実測値**: `Unsafe.SizeOf<S_Greet_for_s_Point>() == 1`（期待値: 0）

**影響**: `DynRef<TVtable>` は意図より 1 バイト大きい。JIT はインライン化で仮想ディスパッチを排除できるため
「呼び出しコスト零」の目標は達成されるが、メモリレイアウトは Rust の ZST とは異なる。

**回避策候補**: .NET 8+ では `[StructLayout(LayoutKind.Sequential, Size = 0)]` で強制的に
零サイズにできるが、netstandard2.0 ターゲットでは使用不可。

### 問題点3: `MemoryMarshal.CreateSpan` が netstandard2.0 のコンパイル時 API に存在しない

`System.Memory` 4.5.5 の実行時バイナリには `MemoryMarshal.CreateSpan<T>(ref T, int)` が含まれるが、
.NET SDK が提供する `netstandard2.0` 参照アセンブリには露出していない。
そのため `LenRef<T>.AsSpan()` と `LenPointer<T>.AsSpan()` は実装できず、省略した。

生成コードはインデックスアクセス（`GetElement(i)`）を使用するため、`AsSpan()` がなくても機能する。

### 問題点4: スタック参照の GC 安全性

`Ref<T>` のスタック参照モード（`Target == null`）では `Offset` に絶対アドレスを格納する。
この参照は対象のスタックフレームが生きている間のみ有効。

コンパイラは以下の制約を強制する必要がある（Rust の借用チェッカーが保証する範囲）:
- `await` をまたがない
- クロージャにキャプチャされない  
- 関数から return でエスケープしない

これらの制約を破ると未定義動作（dangling pointer）になる。

### 問題点5: 二重 panic の動作が Rust と異なる

Rust では Drop 内で panic すると **abort** になる。
C# では `finally` ブロック内で `throw` すると元の例外が破棄される（C# の仕様）。

これは意図的な非再現として設計文書に記録済み。

## Rust コンパイラ実装 (`src/`)

### 実装済みモジュール

| ファイル | 内容 |
|---|---|
| `build.rs` | sysroot lib パスを Cargo に通知（`rustc_private` 使用に必要） |
| `src/main.rs` | `rustc_driver` ベースのエントリポイント、THIR アクセスフック |
| `src/codegen/naming.rs` | 命名規則変換関数（11 テスト） |
| `src/codegen/writer.rs` | `CsWriter` — インデント付き C# コード出力ヘルパー（10 テスト） |

### 問題点6: `rustc_driver` API が nightly 間で頻繁に変更される

nightly-2026-04-20 では以下の API が変更されていた:

| 旧 API | 新 API |
|---|---|
| `RunCompiler::new(&args, &mut cb).run()` | `run_compiler(&args, &mut cb)` |
| `init_rustc_env_logger(None)` | `init_rustc_env_logger(&EarlyDiagCtxt::new(...))` |
| `Callbacks::after_analysis(queries: &Queries)` | `Callbacks::after_analysis(tcx: TyCtxt)` |
| `queries.global_ctxt().unwrap().enter(\|tcx\| {...})` | `tcx` が直接渡される |
| `catch_with_exit_code(...) -> i32` | `catch_with_exit_code(...) -> ExitCode` |
| `ItemKind::Fn(..)` (tuple) | `ItemKind::Fn { ident, sig, .. }` (struct) |
| `Item::ident` フィールド | `ItemKind` の各バリアント内の `ident` |

安定した ABI がないため、コンパイラは固定 nightly バージョン（`.rust-toolchain.toml`）に
ピン留めする必要がある。

### 問題点7: `rustc-dev` コンポーネントは自動インストールされない

`.rust-toolchain.toml` の `components` に `rustc-dev` を記載しても、
rustup は `rustup show` 時に自動インストールしないことがある。

**回避策**: CI/CD で明示的に `rustup component add rustc-dev` を実行する。
`build.rs` は sysroot のパスを設定するが、コンポーネント自体のインストールは行わない。

### TODO: THIR ウォーク・C# コード生成

現在 `compile_fn()` は THIR のメタ情報（expr 数・stmt 数・param 数）をログ出力するのみ。
次のステップ:

1. 型マッピング (`Ty<'tcx>` → C# 型名文字列)
2. 式コンパイル（THIR `ExprKind` → C# 式）  
3. 文コンパイル（THIR `StmtKind` → C# 文）
4. Drop 挿入（スコープ終端での `try/finally` 生成）
5. async/await 変換（THIR の `Await` expr → C# `await`）
