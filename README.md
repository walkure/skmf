# skmf

[大学生協プリペイド](https://mp.seikyou.jp/mypage/)の履歴を[Moneyforward ME](https://moneyforward.com/)へ登録します。

## build

`cargo build --release`

## install

- バイナリを適当な実行フォルダに置く。
- 設定ファイル(config.toml)を書く。
- systemdかなんかで自動実行させる
  - AM1:00とかでよさそう。
  - systemdサンプルファイル参照

引数なしで起動する場合、設定ファイルは実行時のカレントディレクトリに存在してると期待します。
それ以外の場所にある場合は、`--config /usr/local/etc/skmf.toml` のように指定してください。


項目は`config.toml-dist`を参照してください。

### 登録先について

Moneyforward MEで「未対応のその他保有資産」という非対応のクレカや電子マネー情報を入力するカテゴリに登録することを想定しています。
ここでの登録名を`mf_subaccount`に書いてください。

### 登録分類について

今のところ一つしか想定していません。わたしが食堂の支払いにしか使っていないからです。

## セッションについて

Moneyforwardは毎回ログインするたびにログイン通知メールを送ってきてつらいのでセッションCookieを保存しています。
実行時のカレントディレクトリに読み書きします。このデータは普通のjsonなので、設定ファイルとともに
他の人が読めないような場所に置いてください。

## DISCLAIMER

これは作者が勝手に作っているので、大学生協やマネーフォワードとは無関係です。

## Known bugs

生協ポイントの100ポイント自動チャージはWeb上のデータが100円入金と全く同じで区別できないので自動対応を諦めました。

## LICENSE

MIT

## Author

walkure < walkure at 3pf.jp >


