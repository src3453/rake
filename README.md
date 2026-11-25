# Rake

Scratch 3向けの静的型付きスクリプト言語コンパイラです。Rust製の単一クレートからScratch標準ブロックを含む `.sb3` を生成します。コンパイルエラーは `ariadne` でソース位置と該当箇所を表示します。

## ビルドと実行

Rust stableとCargoを使います。Rakeのソース拡張子は `.rk` です。シーン定義には `.rks` を使う予定ですが、シーンDSLはまだ未実装です。

```sh
cargo test
cargo run -- examples/basic.rk -o outputs/basic.sb3
```

生成した `outputs/basic.sb3` はScratch 3エディターで開けます。プログラムは緑旗イベント1つにコンパイルされます。

## VS Code拡張

`editors/vscode` に `.rk` 用のシンタックスハイライト、スニペット、コンパイラー診断、基本補完を備えた拡張機能があります。Language Serverはこのクレートのパーサーと型検査を再利用します。ビルドと起動方法は [拡張機能のREADME](editors/vscode/README.md) を参照してください。

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

`int`、`float`、`bool`、変数、型注釈と型推論、算術・比較・論理演算、`if`、`while`を実装しています。`;` は任意です。`while` はScratchの「〜まで繰り返す」ブロックと条件反転へ変換します。

変数の格納先は宣言位置で分けます。モジュール直下の変数はScratchのStage共有変数へ、関数内や制御ブロック内の変数は実行先Spriteの変数へ割り当てます。関数引数はScratchカスタムブロック引数です。Sprite変数は呼び出しごとのフレームではないため、同じSprite上で複数のスクリプトが並行して同じ関数を使う場合のローカル状態分離は今後の対応項目です。

`fn` の関数宣言と呼び出し（`int` / `float` / `bool` の引数と戻り値）はScratchカスタムブロックへ変換します。戻り値は `fn add(a: int, b: int) -> int` のように型を宣言し、関数本体で `return a + b` と書きます。戻り値はコンパイル時に自動生成されるScratch変数へ保存され、式の中でも呼び出せます。戻り値あり関数はすべての経路で `return` が必要です。戻り値あり関数の再帰呼び出しは未対応です。イベント宣言、文字列、配列、`.rks` シーンDSL、`for` は未対応です。コンパイラは独自VMを出力せず、Scratch標準ブロックへ直接変換します。

クラスは、モジュール直下で静的に宣言するインスタンスと、`int` / `float` / `bool` フィールドの読み書きを実装しています。フィールド初期値は数値または真偽値のリテラルです。各インスタンスのフィールドは個別のStage共有変数に展開します。

```text
class Counter {
    var value: int = 0
    const step: int = 1
}

var counter = Counter()
counter.value += counter.step
```

複数インスタンスのフィールドが独立する例は [examples/objects.rk](examples/objects.rk) を参照してください。

クラスメソッド、実行時のインスタンス生成、クラス型の明示的な型注釈、継承、配列フィールドは未対応です。インスタンス変数自体はScratch変数として出力せず、各フィールドの格納場所へLoweringします。

## Scratchソース

Scratch Foundationの現行モノレポから `packages/scratch-vm` を取得しています。コミットとライセンスは [SCRATCH_SOURCE.md](SCRATCH_SOURCE.md) を参照してください。
