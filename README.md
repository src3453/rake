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

関数、イベント宣言、文字列、配列、クラス、`.rks` シーンDSL、`for` は未対応です。コンパイラは独自VMを出力せず、Scratch標準ブロックへ直接変換します。

## Scratchソース

Scratch Foundationの現行モノレポから `packages/scratch-vm` を取得しています。コミットとライセンスは [SCRATCH_SOURCE.md](SCRATCH_SOURCE.md) を参照してください。
