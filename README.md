# Rake

Scratch 3向けの静的型付きスクリプト言語コンパイラです。Rust製の単一クレートからScratch標準ブロックを含む `.sb3` を生成します。コンパイルエラーは `ariadne` でソース位置と該当箇所を表示します。

## ビルドと実行

Rust stableとCargoを使います。Rakeのソース拡張子は `.rk`、シーン定義（Rake Scene）の拡張子は `.rks` です。

```sh
cargo test
cargo run --bin rake -- examples/basic.rk -o outputs/basic.sb3
cargo run --bin rake -- examples/match.rk -o outputs/match.sb3
cargo run --bin rake -- examples/scratch_lists.rk -o outputs/scratch_lists.sb3
cargo run --bin rake -- examples/events.rk -o outputs/events.sb3
cargo run --bin rake -- examples/macros.rk -o outputs/macros.sb3
cargo run --bin rake -- examples/basic.rk --emit-ir outputs/basic.ir
cargo run --bin rake -- examples/basic.rk --emit-json outputs/basic.json
cargo run --bin rake -- ide/templates/default/scenes/main.rks -o outputs/scene.sb3
```

生成した `.sb3` はScratch 3エディターで開けます。`event` 宣言ごとに独立したScratchスクリプトを生成します。同じイベントやメッセージを受け取るハンドラーを複数宣言できます。イベント外に書いた実行文は、明示した`Scratch.Events.greenFlag`ハンドラーが1つあればその本体の前に連結し、なければ暗黙の緑旗スクリプトにします。

`--emit-ir <path>` は型検査と関数呼び出しのLowering後にバックエンドへ渡す構文木をデバッグ用テキストとして書き出します。`--emit-json <path>` は `.sb3` に格納するプロジェクトJSONを整形して書き出します。どちらも `-o` / `--output` と併用でき、指定した出力だけを作る場合は `-o` を省略できます。例: `cargo run --bin rake -- examples/basic.rk --emit-json outputs/basic.json`

## Rake IDE

[ide/](ide/README.md) にTurboWarpエディターをベースにしたIDEがあります。ブロック・コードエディター（LSP対応）・ステージを並べ、編集から一定時間後に自動でコンパイルしてステージへ反映します。プロジェクト管理と、ステージとシーンファイルの相互変換を含むシーン管理もできます。

## sb3からの逆コンパイル（実験的）

既存の `.sb3` をRakeプロジェクト（`scenes/main.rks`、ターゲットごとの `src/*.rk`、`assets/`、`rake-ide.json`）に変換できます。出力先が空でないときは `--force` が必要です。

```sh
cargo run --bin rake -- decompile game.sb3 -o decompiled/game
cargo run --bin rake -- decompiled/game/scenes/main.rks -o outputs/game.sb3
```

ブロックはコンパイラと同じ組み込みカタログで逆引きするため、出力した呼び出しはそのままコンパイルできます。数値だけを保持する変数は `float`、それ以外は `Scratch.Generics.Value` として宣言し、数値同士の演算は `+` や `<` などの演算子、それ以外は `Operators.add()` などの呼び出しで表します。「ずっと」は `while true`、「〜回繰り返す」はカウンター変数付きの `while`、カスタムブロックは `fn`（画面を再描画せずに実行する場合は `warp fn`）になります。

Rakeで表せないものは近い形に変換し、警告を表示します。

- Rakeで表せないブロックは `// [decompile]` コメントに置き換え、未対応のハットを持つスクリプトはコメントアウトして残します。
- Rakeの識別子はASCIIのみなので、変数名・Sprite名・メッセージ名は変換し、元の名前をコメントに残します。Sprite名の変更はメニューの参照にも反映します。コスチューム名と音の名前はファイル名になるため、ファイル名に使えない文字は置き換えます。
- 「このスプライトのみ」の変数とリストはStage変数（共有）に昇格します。クローンごとに値を持つ動作は失われます。
- モジュール変数の宣言は緑の旗で初期値を代入するため、保存されていた値で毎回初期化されます。Rakeでは変数宣言と複数の `greenFlag` ハンドラーを同じファイルに置けないので、2つ目以降の緑の旗スクリプトは最初のハンドラーから `send rake_green_flag_N` で起動します。
- リストの初期項目、クラウド変数、モニター、拡張機能のブロックは復元しません。

## シーン（.rks）

`.rks` は1つのScratchプロジェクト（Stageと複数のSprite）を宣言します。スクリプト本体は `.rk` に書き、`scripts` でどのターゲットにコンパイルするかを指定します。パスは `.rks` ファイルのあるディレクトリからの相対パスです。

```rust
scene Game {
    stage {
        backdrops: ["../assets/bg.svg", ("../assets/night.png", 240, 180, 2)]
        current_backdrop: "bg"
        scripts: ["../src/stage.rk"]
    }

    sprite Player {
        costume: "../assets/player.svg"
        position: (0, -40)
        size: 100
        direction: 90
        visible: true
        layer: 2
        scripts: ["../src/player.rk"]
        sounds: ["../assets/pop.wav"]
    }
}
```

Stageのプロパティは `backdrop` / `backdrops`、`current_backdrop`、`sounds`、`scripts`、`volume`、`tempo` です。Spriteのプロパティは `costume` / `costumes`、`current_costume`、`sounds`、`scripts`、`position`、`size`、`direction`、`visible`、`layer`、`draggable`、`rotation_style`（`"all around"` / `"left-right"` / `"don't rotate"`）、`volume` です。コスチュームは `"path"` か `(path, 中心X, 中心Y[, bitmapResolution])` で書き、中心を省略すると画像の中央になります。コスチュームと音の名前はファイル名（拡張子なし）で、`current_costume` には0始まりの番号か名前を指定します。対応形式は画像がSVG/PNG/JPEG/GIF/BMP、音がWAV/MP3です。コスチュームを省略したSpriteには既定の円、背景を省略したStageには白い背景を使います。`layer` が大きいほど手前に描画し、省略時は宣言順です。

コンパイラーは、シーン宣言が1つだけであること、Sprite名の重複、未知のプロパティ、値の型と範囲、参照ファイルの存在を検査します。各 `.rk` は独立にコンパイルしてから割り当て先ターゲットへ結合します。複数ソースで同名のモジュール直下変数は1つのStage変数として共有し、同名のメッセージは同じbroadcastになります。同じターゲットで同名の関数（カスタムブロック）を定義すること、Spriteのローカル変数とStage変数の名前の衝突、Stageスクリプトでの動き・見た目などSprite専用ブロックの使用はエラーです。`main.rks` によるシーン遷移、Prefab、Clone定義はまだ未対応です。

`--print-scene` はシーンを解析したJSONを、`--json-errors` はエラーを1行のJSONとして出力します（IDEが使用）。

## VS Code拡張

`editors/vscode` に `.rk` と `.rks` 用のシンタックスハイライト、スニペット、コンパイラー診断、基本補完を備えた拡張機能があります。Language Serverはこのクレートのパーサーと型検査を再利用します。ビルドと起動方法は [拡張機能のREADME](editors/vscode/README.md) を参照してください。

拡張機能の **Rake: Open Scratch Blocks Preview** コマンドを使うと、VS Codeの`.rk`エディターの隣にコンパイル後のScratchブロックを表示できます。編集に合わせてプレビューが更新されます。

## 初版の構文

```text
var score: int = 0
const limit: int = 10
var ready: bool = true

if ready && score < limit {
    score += 1
} else {
    score = 0
}

while score < limit {
    score = score + 1
}
```

`int`、`float`、`bool`、`text`、`Scratch.Generics.Value`、変数、型注釈と型推論、算術・比較・論理演算、`if`、`while`、`match`を実装しています。`;` は任意です。`while` はScratchの「〜まで繰り返す」ブロックと条件反転へ変換します。

## 演算子と型変換

算術演算子は `+`、`-`、`*`、`/`、`%` です。`/` の結果は常に `float`、それ以外は両辺が `int` なら `int` になります。`%` はScratchの「〜を〜で割った余り」ブロックで、結果は割る数と同じ符号です（`-7 % 3` は `2`）。`text` 同士の `+` は「〜と〜」ブロックで連結します。`text` と数値を `+` でつなぐことはできないので、`text(...)` で変換してください。

`text[i]` で0始まりの位置 `i` の文字を `text` として取り出します（`"Rake"[0]` は `"R"`）。Scratchの「〜の〜番目の文字」ブロック `letterOf(i + 1, text)` に変換し、添字がリテラルならコンパイル時に `+ 1` します。添字は `int`、対象は `text` か `Scratch.Generics.Value` です。範囲外は空文字列になり、`text[i] = ...` のような代入はできません。

代入には `=` のほか複合代入 `+=`、`-=`、`*=`、`/=`、`%=` と、1増減する `x++` / `x--`（文として使用、`obj.field++` も可）を使えます。複合代入の結果は通常の演算子と同じ型規則で検査するため、`int` 変数に `/=` は使えません。

組み込みの変換関数 `int(x)`、`float(x)`、`text(x)`、`bool(x)` で型を変換します。

| 変換 | 受け付ける型 | Scratchでの動作 |
| --- | --- | --- |
| `int(x)` | `float`、`text`、`bool`、enum、`Scratch.Generics.Value` | 「切り下げ」ブロック（負の無限大方向へ丸める）。数値でない文字列は `0`、`true` は `1` |
| `float(x)` | `int`、`text`、`bool`、enum、`Scratch.Generics.Value` | `x + 0`。数値でない文字列は `0` |
| `text(x)` | `int`、`float`、`bool`、enum、`Scratch.Generics.Value` | `x` と空文字列の連結。`true` は `"true"` |
| `bool(x)` | `int`、`float` | `not (x = 0)` |

すでに目的の型である値（`int` → `float` など）にはブロックを追加しません。

```rake
fn toHex(input: int) -> text {
    const digits: text = "0123456789ABCDEF"
    var value: int = input
    var result: text = ""
    while value > 0 {
        result = digits[value % 16] + result
        value = int(value / 16)
    }
    return result
}

var count: int = 0
count++
Scratch.Looks.say("count = " + text(count))
```

全体の例は [examples/operators.rk](examples/operators.rk) を参照してください。

## enum

ペイロードを持たない `enum` を宣言できます。variant は宣言順に `0` からタグが割り当てられ、enum 名と variant 名はコンパイル時に解決されます。Scratch ではタグ数値として保存され、実行時の enum オブジェクトやディスパッチ表は作りません。代入、関数の引数と戻り値、`==` / `!=` に対応し、異なる enum 型同士や数値演算には使えません。

```rake
enum GameState {
    Title,
    Playing,
    Paused,
}

var state: GameState = GameState.Title
if state == GameState.Playing {
    Scratch.Looks.say("Playing")
}
```

enum 定数はクラスのフィールド初期値にも使えます。`match` は enum とスカラー値に使え、arm の値を式の結果として扱います。`_` を使わない enum と bool の match では全variant / 両方の値を網羅してください。ブロック arm は関数から `return` するときに使えます。ペイロード付き variant はまだ未対応です。

```rake
fn score(state: GameState) -> int {
    return match state {
        GameState.Title => 0,
        GameState.Playing => 10,
        _ => -1,
    }
}
```
enum の基本例は [examples/enums.rk](examples/enums.rk)、`match` の式 arm・ブロック arm・文としての利用例は [examples/match.rk](examples/match.rk) を参照してください。

## イベント

トップレベルの`event <event> { ... }`でScratchイベントハットと本体を宣言します。組み込みイベントは`Scratch.Events`からimportして使い、カスタムメッセージは名前をそのままイベント名にします。`send <message>`はそのメッセージをbroadcastします。

```rake
use Scratch.Events as Events

event Events.greenFlag {
    Scratch.Looks.say("Ready")
    send reset
}

event Events.keyPressed("space") {
    Scratch.Looks.say("Jump")
}

event reset {
    Scratch.Looks.say("Reset")
}
```

組み込みイベントは`Scratch.Events.greenFlag`、`keyPressed("...")`、`backdropSwitchedTo("...")`、`spriteClicked`、`cloneStart`です。`use Scratch.Events as Events`、`use Scratch.Events.{greenFlag, keyPressed}`、`use Scratch.Events.keyPressed as key`のようにimportでき、別名も使えます。キーと背景名は文字列リテラルで指定します。カスタムメッセージは`event reset { ... }`で受信し、`send reset`で送信します。同じイベントの受信スクリプトを複数宣言すると、Scratch上にも受信ハットが複数生成されます。イベント本体は独立したScratchスクリプトとして動作し、複数のイベント間に実行順序はありません。イベント外の実行文と複数の`greenFlag`ハンドラーを併用すると初期化順を定められないため、コンパイルエラーになります。

動作例は [examples/events.rk](examples/events.rk) を参照してください。

変数の格納先は宣言位置で分けます。モジュール直下の変数はScratchのStage共有変数へ、関数内や制御ブロック内の変数は実行先Spriteの変数へ割り当てます。関数引数はScratchカスタムブロック引数です。Sprite変数は呼び出しごとのフレームではないため、同じSprite上で複数のスクリプトが並行して同じ関数を使う場合のローカル状態分離は今後の対応項目です。

`fn` の関数宣言と呼び出し（`int` / `float` / `bool` / `text` / enum / `Scratch.Generics.Value` の引数と戻り値）はScratchカスタムブロックへ変換します。戻り値は `fn add(a: int, b: int) -> int` のように型を宣言し、関数本体で `return a + b` と書きます。戻り値はコンパイル時に自動生成されるScratch変数へ保存され、式の中でも呼び出せます。戻り値あり関数はすべての経路で `return` が必要です。戻り値あり関数の再帰呼び出しは未対応です。Rake配列と `for` は未対応です。コンパイラは独自VMを出力せず、Scratch標準ブロックへ直接変換します。

`warp fn updateFast() { ... }` のように `fn` の前へ `warp` を付けると、そのカスタムブロックは画面を再描画せずに実行するScratchのwarpオプションを有効にします。

## マクロ

`macro! name (parameters...) { ... }` で定義し、`name!(arguments...)` で呼び出します。コンパイル前にトークン単位で単純置換するため、引数には式や入れ子の呼び出しを渡せます。引数の個数は検査され、置換後のコードは通常の型検査を受けます。文字列内の文字やコメントは置換しません。マクロはトップレベルに書き、再帰展開は最大64段です。例は [examples/macros.rk](examples/macros.rk) を参照してください。

## 名前空間とScratchブロック

`ns Name { ... }` で関数・クラスをまとめます。名前空間名は `UpperCamelCase` を推奨します。名前空間内の関数は `Gameplay.turnAndMove(10)` のように修飾して呼び出せます。

```rust
use Scratch.Motion as motion
use Scratch.Motion.{move, rotate, rotateCW, rotateCCW}

ns Gameplay {
    fn turnAndMove(steps: int) {
        move(steps)
        motion.rotateCW(15)
        rotate(5) // rotate は rotateCW の別名
        rotateCCW(10)
    }
}

Gameplay.turnAndMove(10)
Scratch.Looks.say("Hello!")
var x: float = Scratch.Motion.xPosition()
```

`use Scratch.Motion` は `Motion` という名前で名前空間を取り込みます。`use Scratch.Motion as motion` で別名を指定できます。`use Scratch.Motion.{move, rotate}` は選んだブロックを直接呼び出せるようにし、`use Scratch.Motion as motion.{move, rotate}` は選んだブロックを `motion.move()` / `motion.rotate()` として使えるようにします。`use Scratch.Motion.move as move` のように単一の関数やクラスに別名を付けることもできます。`use Scratch.Motion.*` は名前空間の直下にある宣言をすべて、末尾の名前で取り込みます。

### ライブラリ

別ファイルの宣言は `from "<パス>" use ...` で読み込みます。パスはプロジェクトルート（`rake-ide.json` があるディレクトリ。なければコンパイルするファイルのディレクトリ）からの相対パスで、`.rk` は省略できます。

```rake
// lib/math.rk
ns MathUtil {
    fn clamp(value: int, low: int, high: int) -> int {
        if value < low { return low }
        if value > high { return high }
        return value
    }
}
```

```rake
// src/player.rk
from "lib/math" use MathUtil.clamp
use Scratch.Events.*

event greenFlag {
    Scratch.Looks.say(clamp(42, 0, 10))
}
```

`from "lib/math" use *` は、ライブラリのトップレベルにある宣言をすべて取り込みます。参照できるのは、そのファイルが直接 `from` したライブラリの宣言だけです（ライブラリがさらに読み込んだものは見えません）。ライブラリに書けるのは `use` / `from` / `ns` / `class` / `enum` / `fn` だけで、循環インポートはエラーになります。出力されるのは使われた関数だけです。シーンで同じスプライトの複数のスクリプトが同じライブラリを使っても、カスタムブロックは1つにまとめられます。詳しい規則は [scratch3_language_feature_spec.md](scratch3_language_feature_spec.md) の 4.7 節を参照してください。

組み込みブロックは `Scratch.Motion`、`Scratch.Looks`、`Scratch.Sound`、`Scratch.Events`、`Scratch.Control`、`Scratch.Sensing`、`Scratch.Operators` にあります。呼び出しは対応する標準 opcode のScratchブロックとして出力し、引数の個数と真偽値入力を型検査します。メニュー入力を取る主要なターゲット選択ブロックとbroadcastも利用できます。回転APIは `rotate()` / `rotateCW()` が時計回り、`rotateCCW()` が反時計回りです。Pen と Music の拡張ブロックは `Scratch.Extension.Pen` と `Scratch.Extension.Music` にあります。拡張ブロックを使うと、コンパイラは `.sb3` の `extensions` に必要な拡張IDだけを記録します。

Scratch と TurboWarp は読み込み可能な拡張機能の範囲が異なるため、ビルド対象ランタイムを指定できます。`.rks` ではシーン直下の `runtime: "scratch"` または `runtime: "turbowarp"` で選び、省略時はScratchです。単体 `.rk` は `--runtime scratch|turbowarp` を指定でき、既定値はScratchです。現状のPen/Musicブロックは両方のランタイムで使える共通ブロックです。TurboWarpのURL指定カスタム拡張は、拡張URLをプロジェクトに渡す仕組みが未実装のため対象外です。

```rake
use Scratch.Extension.Pen as Pen
use Scratch.Extension.Music as Music

event Scratch.Events.greenFlag {
    Pen.clear()
    Pen.setColor("#3366ff")
    Pen.setSizeTo(4)
    Pen.down()
    Scratch.Motion.move(100)
    Pen.up()

    Music.setInstrument(1)
    Music.setTempoTo(120)
    Music.playNoteForBeats(60, 1)
}
```

色パラメーターは `Pen.changeColorParamBy("color", 10)`、`Pen.setColorParamTo("brightness", 80)` のように指定します。Musicのドラムと楽器はブロックのメニュー引数として数値または式を渡します。
完全な例は [examples/extensions.rk](examples/extensions.rk) を参照してください。

ScratchのリストはRake配列とは別の名前付き資源です。`use Scratch.Generics.List as SList` を読み込み、モジュール直下で `var inventory: SList = SList.new()` と作ります。`inventory.add()` / `inventory.get()` のようなメソッド呼び出しをScratch標準のリストブロックへ変換し、`get` は `Scratch.Generics.Value` を返します。詳細と例は [examples/scratch_lists.rk](examples/scratch_lists.rk) を参照してください。

クラスは、モジュール直下で静的に宣言するインスタンス、`public` / `private` のフィールドとメソッド、`self` を使ったメソッド呼び出しを実装しています。アクセス修飾子を省略したメンバーは public です。フィールド初期値には数値、真偽値、文字列リテラル、enum 定数を使えます。`new` は特別な生成処理ではなく、`return self` する通常のメソッドです。クラス型の変数は `var counter: Counter = Counter.new()` のように宣言します。

```text
class Counter {
    public var value: int = 0
    private const step: int = 1

    fn new() {
        return self
    }

    public fn increase() {
        self.value += self.step
    }

    private warp fn reset() {
        self.value = 0
    }
}

var counter: Counter = Counter.new()
counter.increase()
```

複数インスタンスのフィールドが独立する例は [examples/objects.rk](examples/objects.rk) を参照してください。

クラスのインスタンス生成は静的で、モジュール直下で宣言します。コンパイラは各インスタンスのフィールドを別々のScratch変数へ割り当て、メソッドをインスタンスごとのカスタムブロックへ展開します。`private` メンバーは同じクラスのメソッド内からのみ使えます。継承と配列フィールドは未対応です。

## Scratchソース

Scratch Foundationの現行モノレポから `packages/scratch-vm` を取得しています。コミットとライセンスは [SCRATCH_SOURCE.md](SCRATCH_SOURCE.md) を参照してください。
