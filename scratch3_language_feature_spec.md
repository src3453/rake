# Scratch 3向けスクリプト言語：機能要綱

-   文書種別: 初期機能要綱・設計方針
-   状態: Draft
-   作成日: 2026-11-25
-   仮称: Rake（Rust + Scratchの意）

## 1. 概要

本プロジェクトでは、Go・Python・Rustなどに影響を受けた、スクリプトベースの静的型付け・オブジェクト指向プログラミング言語を開発する。

主なコンパイルターゲットはScratch
3.0（`.sb3`）とする。独自VM上でプログラム全体を逐次解釈する方式は基本方針とせず、コンパイル時にプログラム構造を解析し、Scratchのブロック、変数、リスト、カスタムブロック、スプライト、クローンなどへ静的にマッピングする。

言語本体はScratchの実装事情から独立させ、中間表現（IR）を介してScratch向けコードを生成する。これにより、言語機能の拡張や最適化を行いやすくする。

本書は現時点の会話で合意・提案された内容を整理した初期要綱であり、未確定事項は「要検討」として記載する。

## 2. 設計目標

1.  Scratch 3.0で実行可能な`.sb3`プロジェクトを生成する。
2.  Scratchの標準ブロックを可能な限り直接生成し、独自VMによる実行時オーバーヘッドを避ける。
3.  Go/Rust風の読みやすいスクリプト構文と、Python風の扱いやすい開発体験を提供する。
4.  クラス、インスタンス、メソッド、列挙型、パターンマッチなどの高水準機能を提供する。
5.  オブジェクトやローカル変数をScratchの変数・リストに効率よく割り当てる。
6.  Scratchのイベント・協調スレッド実行モデルを言語側から扱いやすくする。
7.  シーン、スプライト、アセットなどを宣言的に記述できるデータ定義言語を提供する。
8.  言語仕様とScratchバックエンドを分離し、将来の別バックエンド追加を妨げない。

## 3. 全体構成

想定するコンパイルパイプラインは次のとおり。

``` text
Source Code (.src)
       |
       v
Lexer / Parser
       |
       v
AST
       |
       v
Name Resolution / Type Checking
       |
       v
Typed IR
       |
       v
Optimization / Lowering
       |
       v
Scratch Backend
       |
       +--> Scratch blocks
       +--> Variables / Lists
       +--> Sprites / Clones
       +--> Assets
       |
       v
.sb3 (ZIP + project.json + assets)
```

シーン定義ファイルは別の入力として読み込み、シーンASTを検査したうえで、プログラム側の型・クラス・アセット参照と結びつける。

### 3.1 フロントエンド

-   字句解析器
-   構文解析器
-   AST
-   名前解決
-   型検査・型推論
-   シーン定義の解析
-   アセット・クラス・シーン間の参照検査

### 3.2 中間表現（IR）

IRはScratchに依存しない表現とする。少なくとも以下を表現できること。

-   基本演算、変数、代入
-   条件分岐、ループ
-   関数定義・呼び出し
-   クラス、フィールド、メソッド
-   列挙型・代数的データ型
-   パターンマッチ
-   イベントハンドラ
-   yield・待機などの協調実行ポイント
-   配列・オブジェクト・クロージャ
-   Mutex、Channelなどの並行処理プリミティブ
-   シーンとリソースの参照

### 3.3 最適化・Lowering

Scratch Backendに渡す前に、可能な限り実行時の抽象化を除去する。

候補となる最適化：

-   定数畳み込み
-   不要コード・不要変数の除去
-   メソッド呼び出し先の静的解決
-   関数・メソッドのインライン展開
-   ローカル変数の割り当て最適化
-   オブジェクト表現の静的化
-   Structure of Arrays（SoA）への変換
-   パターンマッチの条件分岐への変換
-   クロージャの環境データへの変換
-   不要な一時オブジェクトの除去

## 4. プログラミング言語機能

### 4.1 基本構文

Go/Rust風のブレース構文を基本候補とする。

``` rust
const fn main() {
    const SCORE: int = 0
    var lives: int = 3

    if lives > 0 {
        lives -= 1
    }
}
```

初期候補：

-   `const`: 再代入不可の束縛
-   `var`: 再代入可能な変数
-   `fn`: 関数・メソッド定義
-   `if` / `else`
-   `for` / `while` / `loop`
-   `return`
-   `break` / `continue`
-   `import`
-   `class` / `struct` / `enum`
-   `match`
-   `event`
-   `self`
-   `async fn` / `spawn`: 協調実行タスク
-   `command fn` / `send`: アクター間の非同期メッセージ
-   `await`: Channel 等の非同期結果を待つ
-   `warp fn`: 関数に指定するとScratchのカスタムブロックのwarp実行を有効にする

命名規約は推奨スタイルとして次を採用する。コンパイラーは規約違反だけを理由にエラーにせず、必要に応じてFormatterやLintで案内する。

-   関数・メソッド: `camelCase` (`takeDamage`)
-   変数・フィールド: `snake_case` (`player_score`)
-   定数 (`const`): `UPPER_SNAKE_CASE` (`MAX_SCORE`)
-   クラス: `UpperCamelCase` (`PlayerController`)

セミコロンの扱い、式指向にするか文指向にするか、インデント規則などは要検討。

### 4.2 型システム

初期方針は静的型付けとする。ローカル変数などでは型推論を利用できるようにする。

基本型の候補：

-   `int`
-   `float`
-   `bool`
-   `string`
-   `void` またはユニット型

複合型の候補：

-   `Array<T>`
-   タプル
-   `struct`
-   `enum`
-   クラス型
-   関数型
-   `Task<T>` / `Channel<T>`

Scratchの数値表現・文字列表現との違いを吸収するため、型の表現と変換規則を明確に定義する。

`Scratch.Generics.List` と `Scratch.Generics.Value` はScratch固有機能として扱い、`Array<T>` の別名や自動変換先にはしない。Rake配列は言語の汎用コンテナであり、Scratchリストはプロジェクト内に名前を持つ、対象に所有された可視資源である。

### 4.3 クラス・オブジェクト

Smalltalkのオブジェクト指向モデルを参考にしつつ、Scratch上では静的解決を優先する。

``` rust
class Player {
    var x: float = 0.0
    var y: float = 0.0
    var hp: int = 100

    fn move(dx: float, dy: float) {
        self.x += dx
        self.y += dy
    }
}
```

要件・方針：

-   クラスはフィールドとメソッドを持つ。
-   インスタンスは独立した状態を持つ。
-   `self`で現在のインスタンスを参照する。
-   コンパイル時に呼び出し先が確定するメソッドは直接呼び出しに変換する。
-   必要に応じてメソッドをインライン展開する。
-   動的ディスパッチは必要性とコストを検討したうえで導入する。
-   クラス継承は初期段階では必須としない。

Smalltalkのメッセージ送信は設計上の参考とするが、通常のメソッド呼び出しを実行時の汎用ディスパッチで実装することは避ける。

### 4.4 structとimpl

Rust/OCamlに近い値の構造化とメソッド実装も候補とする。

``` rust
struct Vec2 {
    x: float
    y: float
}

impl Vec2 {
    fn lengthSquared() -> float {
        self.x * self.x + self.y * self.y
    }
}
```

クラスとstructの意味上の違い（値型・参照型、同一性、コピー規則）は要検討。

### 4.5 代数的データ型・パターンマッチ

OCamlの代数的データ型（ADT）とパターンマッチを参考にする。

``` rust
enum GameState {
    Title,
    Playing,
    Paused,
    GameOver
}

match state {
    Title => show_title(),
    Playing => update_game(),
    Paused => show_pause(),
    GameOver => show_result()
}
```

Scratch Backendではタグ値と条件分岐などにLoweringする。パターンの網羅性はコンパイル時に検査する。

初期実装ではペイロードを持たないenumを対象とし、variantへ宣言順の0始まり整数タグを割り当てる。enum型とvariant参照はコンパイル時に解決し、実行時にはタグ値を通常のScratch数値変数へ格納する。enum同士の`==` / `!=`、変数代入、関数引数・戻り値、クラスフィールド初期値を扱う。異なるenum型間の比較や数値演算は型エラーとする。`match` は enum variant と scalar literal (`int`、`float`、`bool`、`text`) をパターンにし、arm の値式を式の結果として扱う。enum は全variant、bool は `true` と `false` を列挙するか、最後に `_` を置く。式として使うブロック arm は現在の関数から `return` して終了する。match は判定値の一時変数と条件分岐へLoweringする。ペイロード付きvariantは後続段階とする。

### 4.6 関数・クロージャ

関数を独立した定義単位として扱う。クロージャは将来機能として検討する。

クロージャを実装する場合の候補表現：

``` text
Closure:
    FunctionID
    EnvironmentID
```

キャプチャされた変数は環境データとしてScratch変数・リストへ割り当てる。呼び出し先が静的に確定する場合は直接呼び出しに最適化する。

### 4.7 モジュール

Python風のモジュール分割を提供する。

``` rust
import game.player
import scratch.input
```

ファイル単位のモジュール、可視性、循環import、パッケージ構成は要検討。

## 5. Scratchへのデータマッピング

独自VMで全処理を解釈するのではなく、コンパイル時にScratchの変数・リスト・ブロックへ割り当てることを基本とする。

### 5.1 静的インスタンス

コンパイル時にインスタンス数や参照先を確定できる場合、フィールドを個別のScratch変数へ割り当てる。

例：

``` rust
const PLAYER = Player()
player.x += 10
```

概念的な生成結果：

``` text
player_x
player_y
player_hp
```

フィールドアクセスはScratchの変数アクセス・代入・変更ブロックへ直接変換する。

初期実装では、モジュール直下の `var player = Player()` のようにコンパイル時に特定できるインスタンスを対象とする。クラス本体は `int` / `float` / `bool` / `text` / enum の `var` / `const` フィールドを持ち、初期値は対応するリテラルまたはenum定数とする。インスタンス変数そのものはScratchに作らず、フィールドごとに一意なStage共有変数を割り当てる。動的な実行時生成はこの段階では対象外とする。

### 5.2 動的インスタンス

実行時に生成される多数のインスタンスはScratchリストで管理する。

基本候補はStructure of Arrays（SoA）方式とする。

``` text
Player_x       = [100, 200, 300]
Player_y       = [50, 80, 120]
Player_hp      = [100, 80, 100]
Player_alive   = [1, 1, 1]
```

同じインデックスを同じインスタンスのハンドルとして扱う。

要件：

-   インスタンスIDとリストインデックスの対応を定義する。
-   フィールドアクセスをリスト操作へLoweringする。
-   削除時にリスト途中の要素を安易に削除しない。
-   初期実装では生存フラグを使った論理削除を候補とする。
-   将来的にFree Listによるスロット再利用を検討する。
-   配列・リストの境界チェックと無効ハンドルの扱いを定義する。

### 5.3 オブジェクト管理方式

Arena方式を初期候補とする。オブジェクトIDは整数ハンドルとし、実データは型ごとのリスト群に格納する。

GCは初期必須機能としない。削除・再利用・参照の有効性を明確にする。

### 5.3.1 struct実装計画（着手前）

structは値型として導入する候補だが、Scratch上での動的な格納領域と破棄方法を先に決める。静的インスタンスだけなら各フィールドをScratch変数へ展開できるが、関数・ループ・Cloneで生成される値を同じ仕組みで表すには固定スロット、SoA配列、またはArenaと空きスロット管理が必要になる。次の順で仕様を固めてから実装する。

1. **値の意味を確定する**: コピーと代入、同値比較、可変フィールド、関数引数・戻り値、ネストしたstruct、配列格納、参照の同一性を定義し、classとの差を明記する。
2. **生成と寿命を分類する**: 個数が固定されたモジュール値、関数内一時値、ループ内の繰り返し生成、Clone単位の値、コンテナに格納された値を整理する。値が関数外や別タスクへ逃げる場合の寿命を決め、静的に格納先を決められる範囲と動的確保が必要な範囲を区別する。
3. **Scratch表現を比較する**: 固定個数の値はフィールドごとの変数へ展開する。動的値はSoA形式のScratchリストと整数スロットID、または固定容量Arenaを比較し、メモリ量、アクセスブロック数、上限、確保失敗の診断、再利用コストを実測する。通常のRake配列と名前付きScratchリスト資源は別概念のまま保つ。
4. **解放と協調実行を決める**: 関数終了、代入による置換、Clone削除、Scene遷移でいつスロットを再利用するかを決める。yieldをまたぐ参照、複数スクリプトからの共有、Mutex保持中の確保・解放、Clone間の所有者分離を検討し、二重解放や古いIDの再利用を防ぐ世代番号が必要か判断する。
5. **段階的に実装する**: まずコンパイル時に個数が確定するstructを型検査・IR・Scratch変数へ展開する。次にエスケープ解析と固定容量ArenaまたはSoAを追加し、容量制限と失敗診断を用意する。最後にClone・Scene・yieldを含む寿命検査とLSP表示を同期する。

可変長配列の自動拡張、再帰的なstruct、循環参照、任意の解放タイミングを初期仕様に含めるかは実測と寿命モデルを見て決める。値型コピーを明示できる静的structを最初の実装境界とし、動的リソース管理は別段階にする案を推奨する。

### 5.4 ローカル変数と呼び出しフレーム

Scratch変数へ単純に展開すると、複数イベントやクローンでローカル変数が衝突する可能性がある。

現在の初期割り当てでは、モジュール直下の束縛をStage共有変数、関数内または制御ブロック内の束縛を実行先ターゲットの変数、関数引数をScratchカスタムブロック引数として表す。Scratchのターゲット変数は呼び出しごとのフレームではないため、複数スクリプト間のローカル状態分離とyieldをまたぐ状態は別途Loweringする。

そのため、次を設計する。

-   スプライトごとの変数と全体変数の使い分け
-   イベント・クローンごとの実行状態
-   関数呼び出し時のローカル変数の割り当て
-   再帰呼び出しを許可するかどうか
-   yieldをまたいで生存するローカル変数の扱い

Scratchの変数スコープとクローン時の変数複製動作を踏まえて仕様化する。

### 5.5 Scratch固有リスト

ScratchリストはRake配列と異なる型・資源として扱う。`Scratch.Generics.List` はリストハンドルと操作を提供し、`Scratch.Generics.Value` はリスト要素として保持できるScratch値を表す。`use` で型と操作の名前空間を別名インポートできる。

```rust
use Scratch.Generics.List as SList
use Scratch.Generics.Value as SValue

var inventory: SList = SList.new()
inventory.add("red")
inventory.add(12)
inventory.insert(2, "blue")
inventory.set(1, "green")

var first: SValue = inventory.get(1)
var item_count: int = inventory.length()
var has_blue: bool = inventory.contains("blue")
var blue_index: int = inventory.indexOf("blue")

if has_blue {
    inventory.delete(blue_index)
}

inventory.clear()
inventory.show()
inventory.hide()
```

`SList.new()` はモジュール直下の変数宣言で使い、変数名をScratchリスト名としてStage共有リストを生成する。リストハンドルは再代入できず、関数の引数・戻り値やクラスフィールドには使えない。`add`、`insert`、`set`、`get`、`delete`、`clear`、`length`、`contains`、`indexOf`、`show`、`hide` は対応するScratch標準ブロックへLoweringする。`get` は `SValue` を返し、`get` / `set` / `insert` / `delete` の添字は1始まりとする。`indexOf` は不一致時に `0` を返す。

この初期実装ではStage所有のリストを生成する。Sprite所有リスト、リストの初期要素、Rake配列のBackend表現は別途定義する。

### 5.6 Sprite / Target 参照と呼び出し

Stage と各 Sprite を独立したアクターとする。`.rks` で `.rk` ソースをターゲットに割り当てると、そのスクリプトの `self` は割り当て先アクターを表す。アクターは自分のターゲット状態を所有し、Stage と Sprite では利用できるプロパティ・メソッドが異なるため型検査で区別する。

Scene 内で宣言した Sprite は、`sprites.<名前>` による静的アクター参照として使う。参照先はコンパイル時に解決し、名前が存在しなければエラーにする。初期版では動的に選ぶ Sprite や Clone 個体への参照は扱わない。

Sprite スクリプトでは `self` の座標・向き・大きさを読み書きできる。移動・方向指定・表示などの操作は `self` のメソッドとして書き、Scratch 標準ブロックへ直接変換する。同じアクター内の通常メソッド呼び出しは同期呼び出しとする。

``` rust
self.x = 100
self.y = 20
self.direction = 45
self.size = 100
self.goTo(100, 20)
self.move(10)
self.turn(15)
self.pointTowards(sprites.Enemy)
self.show()
```

ここで `direction` は Scratch の向き（角度）を表す。見た目の回転方式は別設定とする。`self` が Stage の場合、座標・向き・大きさ・移動メソッドは型エラーにする。

別 Sprite の参照は初期版では読み取り専用とし、`x`、`y`、`direction`、`size` などの公開属性を読めるようにする。各読み取りはそのブロックが評価された時点の値を返し、複数フィールドを一括取得する整合スナップショットは保証しない。接触判定、距離、別 Sprite の方向を向く操作も Sprite 参照を引数に取る。

``` rust
if sprites.Enemy.x > self.x {
    self.pointTowards(sprites.Enemy)
}

if self.isTouching(sprites.Enemy) {
    self.goTo(0, 0)
}
```

別 Sprite の属性への代入 (`sprites.Enemy.x = 0`) は許可しない。別 Sprite に処理を依頼する場合は、受信側が公開した `command` に非同期メッセージを送る。

``` rust
command fn takeDamage(amount: int) {
    self.hp -= amount
}

send sprites.Enemy.takeDamage(10)
```

`send` は受信側アクターのメールボックスへ型付きメッセージを送り、呼び出し元は完了を待たずに続行する。`command` は戻り値を持たず、呼び出し元から受信側のローカル状態を直接変更できない。応答が必要な場合は Channel 等を使った要求・応答として定義する。

## 6. 並行実行・スレッド管理

Scratchのイベントスクリプト、クローン、協調実行モデルを言語側から扱えるようにする。

### 6.1 実行モデル

-   Stage と各 Sprite をアクターとし、各ターゲットに割り当てたイベント・関数をそのアクターの協調実行タスクとする。
-   ターゲット状態は所有アクター内で変更する。他アクターへの操作は `send` によるメッセージ送信を基本とする。
-   アクターのメールボックスは到着した順にメッセージを受け付ける。各ハンドラーは独立タスクとして実行され、完了順はyieldや待機の位置によって変わりうる。
-   Scratchのスクリプトは協調的に実行される。
-   `yield()`、待機、イベント待ちなどを実行権の譲渡ポイントとして扱う。
-   通常のOSスレッドやプリエンプティブな並行実行を前提としない。
-   コンパイラはyieldをまたぐ変数の生存期間を考慮する。
-   共有可変状態よりも、アクター内状態とChannelによるメッセージ通信を優先する。Mutexは共有状態が必要な場合に使う。

イベント構文の候補：

``` rust
use Scratch.Events as Events

event Events.greenFlag {
    loop {
        update()
        yield()
    }
}

event Events.keyPressed("space") {
    jump()
}
```

### 6.2 Mutex

Scratch変数を使ったMutexを提供する。

概念的なデータ：

``` text
Mutex:
    locked: bool
    owner: ThreadID
```

API候補：

``` rust
const MUTEX = Mutex()

fn updateSharedState() {
    MUTEX.lock()
    // 共有状態の更新
    MUTEX.unlock()
}
```

設計上の必須事項：

-   ロックの確認と取得の間にyieldを挟まない。
-   `tryLock()`は短い非yield区間で実行する。
-   ロック取得に失敗したスレッドは、busy-waitで実行権を占有せずyieldする。
-   所有者IDを保持し、別スレッドによるunlockを検出する。
-   二重unlockや不正なunlockを検出する。
-   ロック保持中にyieldできるか、その場合のデッドロック対策を定義する。
-   warpモードを利用する場合、その実行上限・yield挙動を検証する。

MutexはScratchの協調実行における共有状態保護機構であり、一般的なOSスレッド向けMutexと同一の意味を持つとは限らない。

### 6.3 タスクとChannel

`async fn` とイベントハンドラーは協調実行タスクとして動作する。`spawn` は新しいタスクをスケジューラーへ登録し、`await` は現在のタスクを一時停止して結果を待つ。待機中に他のタスクやアクターは実行を続ける。

`Channel<T>` をタスク間通信の基本プリミティブとする。Channel は型付き値を運ぶキューであり、受信側は値が届くまで yield する。容量、close、複数Channelを待つ `select` の仕様は別途定める。アクターへの `send` はアクターのメールボックスにコマンドを配送し、Channel の送受信とは区別する。

共有可変状態を直接操作する設計を避け、アクターのメールボックスと Channel によるメッセージ通信を優先する。その他の並行処理プリミティブは将来候補とする。

  機能           目的
  -------------- --------------------------
  `Atomic<T>`    単純な共有値の操作
  `Semaphore`    同時実行数の制限
  `WaitGroup`    複数タスクの完了待ち

## 7. シーン定義DSL

通常の Scene 用 `.rks` は Scratch プロジェクト内の1シーンを宣言的に記述し、1ファイルに1つのシーンを定義する。`.rks` はランタイム選択以外のプロジェクト全体のビルド設定や任意の実行コードを含めない。プロジェクトルートの `main.rks` は初期 Scene を定義し、Scene 間の遷移を記述する特別な入力とする。

シーンには Scratch の Stage と Sprite を含める。Stage は背景だけでなくスクリプトの割り当て先にもなる。スクリプト本体は `.rk` に書き、`.rks` ではどの Scratch ターゲットに割り当てるかを宣言する。

Rake のコンパイラーとは別の crate として Cargo のようなビルドシステムを作る予定である。ビルドシステムがパッケージ設定と入力ファイルを解決し、コンパイラー crate が `.rk` と `.rks` を Scratch プロジェクトへ変換する。ビルドマニフェストの形式や成果物の配置はビルドシステム側の仕様で定める。

`.rks` は Rust 風の宣言型 DSL とし、スクリプトコードを埋め込まず、シーン構成と `.rk` ソースへの参照を記述する。

### 7.1 ファイル構成

``` text
project/
├── <build-system manifest>
├── main.rks              # optional scene-transition entry
├── src/
│   ├── stage.rk
│   └── player.rk
├── scenes/
│   ├── title.rks
│   └── game.rks
└── assets/
    ├── player.png
    └── bg.png
```

各 Scene 用 `.rks` は単一の Scene を定義する。ビルドマニフェストは別 crate が所有する別ファイルであり、`.rks` をパッケージマニフェストとして扱わない。

### 7.2 ランタイムと拡張機能

Scratch と TurboWarp では利用できる拡張機能が異なるため、`.rks` の Scene 直下に `runtime: "scratch"` または `runtime: "turbowarp"` を指定する。省略時は Scratch を対象とする。単体 `.rk` のコンパイルでは `--runtime scratch|turbowarp` で指定し、省略時は Scratch とする。

組み込み拡張ブロックは `Scratch.Extension.<ExtensionName>` 名前空間に置き、ブロックカタログには Scratch の拡張IDと対応ランタイムを記録する。生成する `.sb3` の `extensions` には、実際に使用した拡張IDだけを出力する。Pen と Music は両ランタイム共通の拡張として提供する。TurboWarp のURL指定カスタム拡張は、拡張のURLをプロジェクトへ渡す仕組みを別途実装するまで対象外とする。

### 7.3 シーン定義

``` rust
scene Game {
    stage {
        backdrop: "../assets/bg.png"
        scripts: ["../src/stage.rk"]
    }

    sprite Player {
        costume: "../assets/player.png"
        position: (0, 0)
        size: 100
        direction: 90
        visible: true
        layer: 10
        scripts: ["../src/player.rk"]
    }
}
```

この構文例は仕様案であり、正式なトークン文法は別途定義する。アセット、`.rk` ソース、Scene 参照の相対パスは、その参照を記述した `.rks` ファイルのあるディレクトリを基準に解決する案とする。

Scene は Scratch の Stage ターゲットを1つ持ち、0個以上の Sprite ターゲットを持つ。`stage` と `sprite` の `scripts` は `.rk` ソースのリストであり、そこに書かれたスクリプトをそれぞれ対応する Scratch ターゲットに生成する。Stage は Scratch と同じく座標・大きさ・向きを持たない。1つの `sprite` 宣言は1つの Scratch Sprite 定義を表し、Clone の初期生成数を表すものではない。

初期版で扱う情報：

-   背景
-   Sprite 定義とコスチューム・アセット参照
-   Sprite の初期座標・大きさ・向き・表示状態
-   Stage と Sprite それぞれへの `.rk` スクリプト割り当て
-   Sprite のレイヤー・表示順序

複数のコスチュームや背景、初期変数、Prefab、Clone、衝突グループは後続仕様で定める。

### 7.3 シーン遷移とビルド出力

プロジェクトルートの `main.rks` は、通常の Scene 用 `.rks` と同じく初期 Scene を1つ定義し、さらに専用の `transition` 宣言で Scene 間の遷移を記述する。ビルドシステムは初期 Scene と遷移先 Scene をまとめて1つの `.sb3` にする。`main.rks` がない場合は、各 Scene 用 `.rks` をそれぞれ独立した `.sb3` にする。

`transition` はシーン遷移専用の宣言であり、遷移先 Scene を明示する。文法の骨格は `transition <from-scene> -> <to-scene> ...` とする。遷移条件の記法と発火タイミングは `.rk` のイベント機能と合わせて定義する。遷移元・遷移先の Scene 名は、プロジェクト内の Scene 用 `.rks` で解決できなければならない。

### 7.4 シーンとプログラムの静的検査

コンパイラは次を検査する。

-   Scene 用 `.rks` にシーン宣言がちょうど1つあること
-   Sprite 名がシーン内で一意であること
-   `main.rks` がある場合、初期 Scene が定義され、全 `transition` の遷移元・遷移先が解決できること
-   参照された `.rk` ソースとアセットが存在すること
-   値の型と範囲が対応する Scratch ターゲットの制約に適合すること
-   `.rk` ソースのスクリプトを割り当て先の Stage または Sprite に生成できること

### 7.5 Scene / Sprite / Prefab / Instance

次の概念を分離する。Scene は Stage と Sprite の配置・初期状態を束ねる単位であり、Stage と各 Sprite はスクリプトを持てる Scratch ターゲットに対応する。

  概念          役割
  ------------- --------------------------------------------------
  Scene         ゲームの状態、配置、環境
  Sprite Type   Scratch上のスプライト定義・振る舞いの単位
  Class         言語上のデータ構造と振る舞い
  Prefab        アセット・初期値・生成設定をまとめたテンプレート
  Instance      実行時に存在する個々のオブジェクト

ScratchのCloneはInstanceの実装手段の一つとする。ただし、大量のオブジェクトではSoAによる独自管理も選択できるようにする。Clone と Prefab は `.rks` 初期版の対象外とする。

### 7.6 Prefab

``` rust
prefab Enemy {
    sprite: "enemy.png"
    class: EnemyController

    defaults {
        hp: 100
        speed: 2.0
    }
}
```

シーンからの生成例：

``` rust
scene Game {
    Enemy.spawn(100, 50)
    Enemy.spawn(200, 80)
}
```

Prefabはクラスそのものではなく、生成時のアセット・初期値・設定をまとめる概念とする。

## 8. Scratch Backend

出力形式はScratch 3.0の`.sb3`とする。

生成対象：

-   `project.json`
-   Scratchのブロックグラフ
-   変数・リスト定義
-   Stage・Sprite・Clone関連の構造
-   コスチューム・音声などのアセット
-   必要なメタデータ

基本方針：

-   Scratch標準ブロックに対応する処理は標準ブロックへ変換する。
-   関数はScratchのカスタムブロックなどへ変換する。
-   イベントは対応するイベントハットブロックへ変換する。
-   クラスやデータ構造は変数・リストへLoweringする。
-   実行時VMは原則として使用しない。
-   Scratch標準機能で表現しにくい機能は、生成コード量・速度・保守性を比較して実装方式を決める。

## 9. 開発段階

### Phase 1：最小コンパイラ

-   Lexer / Parser
-   AST
-   整数・浮動小数点・真偽値
-   変数と代入
-   四則演算・比較演算
-   if / else
-   while / for
-   Scratchブロック生成
-   `.sb3`出力
-   Scratch 3のエディターと並べて開発ができるIDEを用意する

### Phase 2：関数とイベント

-   関数定義・呼び出し
-   引数・戻り値
-   Scratchカスタムブロックへの変換
-   イベントハンドラ
-   yield・wait
-   ローカル変数のスコープ管理

### Phase 3：オブジェクトモデル

-   class / struct
-   フィールド
-   メソッド
-   self
-   静的メソッド解決
-   静的インスタンスの変数展開
-   動的インスタンスのSoA管理

### Phase 4：型システムとデータ型

-   型推論
-   enum / ADT（初期enumは実装済み。ペイロード付きvariantは未実装）
-   match
-   網羅性検査
-   Array`<T>`{=html}
-   `Scratch.Generics.Value` / `Scratch.Generics.List` とRake配列の意味上の分離
-   タプル
-   モジュール

### Phase 5：シーンDSL

-   プロジェクト定義
-   シーン定義
-   スプライト・アセット参照
-   初期配置
-   クラスとの紐付け
-   Prefab
-   シーン間遷移

### Phase 6：並行処理

-   スレッド・タスクの概念
-   Mutex / tryLock
-   所有者ID
-   Channel
-   WaitGroup
-   クローンごとの状態管理
-   デッドロック検出・診断

### Phase 7：高度な機能・最適化

-   クロージャ
-   インライン展開
-   不要コード除去
-   オブジェクト割り当て最適化
-   Free List
-   動的ディスパッチ（必要な場合）
-   デバッグ情報・ソースマップ

## 10. 要検討事項

以下は実装着手前または該当機能の実装前に決定する。

1.  言語名・ファイル拡張子・予約語
2.  式指向か文指向か、セミコロンや改行の規則
3.  `class`と`struct`の意味上の違い
4.  値型・参照型・コピー規則
5.  クラス継承・trait/interfaceの必要性
6.  動的ディスパッチの範囲
7.  クロージャとキャプチャの仕様
8.  再帰関数を許可するか
9.  Scratchのカスタムブロックの引数・戻り値・warp制約
10. クローンとSoA管理を切り替える基準
11. 文字列・Rake配列・Scratchリスト・辞書の内部表現と変換境界
12. Mutex保持中のyieldを許可するか
13. Scratchのwarp実行制限と長時間処理への対処
14. Scene DSLの具体的な文法と初期化式の範囲
15. アセットのインポート・変換・重複排除
16. Scratch以外のバックエンドを将来サポートするか

## 11. 設計上の基本原則

-   高水準の言語機能はIRまで保持し、Scratch固有の表現への変換はBackendで行う。
-   実行時の汎用処理より、コンパイル時の静的解決・展開を優先する。
-   Scratchの標準ブロックで効率よく表現できる処理は、標準ブロックへ直接変換する。
-   動的なデータ構造はScratchの変数・リストで表現し、VMによる逐次解釈を避ける。
-   Scratchの協調実行モデルを前提に、yieldポイントと共有状態を明示的に扱う。
-   Scene
    DSLは宣言的に保ち、ゲームロジックは通常のプログラミング言語側に置く。
-   Scratchの制約や仕様に依存する処理をフロントエンドへ持ち込まない。
-   性能上の判断は実際に生成したScratchプロジェクトで計測する。

------------------------------------------------------------------------

## 付録A：参考にする言語・システム

  参考対象                  取り入れたい要素
  ------------------------- -----------------------------------------------
  Go                        シンプルな構文、関数、並行処理の考え方
  Python                    モジュール構成、書きやすさ
  Rust                      型宣言、struct、impl、明示的なデータ設計
  Smalltalk                 オブジェクト指向、メッセージ送信、クロージャ
  OCaml                     型推論、代数的データ型、パターンマッチ
  Godot等のゲームエンジン   Scene、Prefab相当のデータ定義
  Scratch 3.0               実行ターゲット、ブロック・変数・リスト・Clone

## 付録B：用語

-   **AST**: Abstract Syntax Tree。構文解析後の抽象構文木。
-   **IR**: Intermediate Representation。コンパイラ内部の中間表現。
-   **Lowering**:
    高水準の構文やデータ構造を、より低水準の表現へ変換すること。
-   **SoA**: Structure of
    Arrays。フィールドごとに配列を分けてデータを格納する方式。
-   **Arena**: オブジェクトをまとまった領域・スロット群で管理する方式。
-   **Clone**: Scratchにおけるスプライトの複製インスタンス。
-   **Warp**:
    Scratchのカスタムブロック等で利用される、画面更新を抑えて実行する仕組み。
-   **協調実行**:
    実行中の処理が明示的な待機・yieldポイントで実行権を譲る方式。

## Initial macro support

Rake supports top-level C-style token substitution macros using `macro! name (parameters...) { ... }` and `name!(arguments...)`. Expansion happens before parsing and type checking; arguments are split while respecting nested parentheses/braces. This initial form has no token pasting or stringification. Recursive expansion is limited to 64 levels. See `examples/macros.rk`.
