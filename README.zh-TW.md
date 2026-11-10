# elden-ring-harness

法環的模組，把遊戲內部接出來給實驗腳本用：記錄主執行緒每一幀把時間花在哪、傳送到固定的
起點、重置 boss、重播錄好的路線，讓掉幀能重現，並從遊戲內部找出原因。

狀態：第一步（記錄主迴圈每個 task group 的時間）可以編譯，還沒在遊戲裡跑過。
模組依靠的東西整理在 [docs/research.md](docs/research.md)。

## 只在實驗時使用

平常玩不會載入這個模組。從 Steam 啟動會走 `start_protected_game.exe`，EAC 開著，遊戲維持原狀。
做實驗時改用 me3 模組載入器和專用的 profile 啟動，離線、使用另一個存檔。

## 建置

DLL 的目標平台是 Windows，在 Linux 上用 [cargo-xwin](https://github.com/rust-cross/cargo-xwin) 編譯：

```sh
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

## 執行

先安裝 [me3](https://github.com/garyttierney/me3)、備份存檔，然後：

```sh
./launch.sh                                  # 寫到 out/<日期-時間>/
ERH_GROUPS=FrameBegin,DrawStep ./launch.sh   # 要放標記的 task group，或 `all`
```

`launch.sh` 會設好 `ERH_OUT`，並且在這個目錄以外啟動 me3：me3 會優先從
`./target/x86_64-pc-windows-msvc/release` 找它自己的 Windows 執行檔，在這裡會找到本專案的
編譯結果，遊戲就開不起來。

me3 會讓遊戲保持離線，並改用存檔 `ERH0000.sl2`（第一次啟動時從目前的存檔複製）。
沒有設 `ERH_OUT` 時，模組什麼都不做。

`out/` 裡的檔案：

| 檔案 | 欄位 | 內容 |
|---|---|---|
| `meta.tsv` | `key value` | 遊戲的 Linux pid、QueryPerformanceCounter 的頻率 |
| `groups.tsv` | `group name` | 每個 task group 的編號和名字，順序就是遊戲執行的順序 |
| `tasks.tsv` | `t_ns tid group` | 標記 task 每執行一次一列：QueryPerformanceCounter 換成 ns、Linux tid、task group |

模組在 `ERH_GROUPS` 列出的每個 task group 裡註冊一個標記 task（預設只有 `FrameBegin`，
名字可以在 `groups.tsv` 查）。每一幀的 group 都照固定順序執行，所以兩個相鄰標記之間的時間差，
大約就是中間那些 group 花的時間。同一個 group 裡的 task 順序不固定，所以標記可能落在 group
裡的任何位置。

放標記本身會改變量到的結果：標記本身約 2µs，但遊戲看起來沒在用的 group（例如 debug 用的那些）
放了標記之後，每幀各花約 150µs。`ERH_GROUPS=all` 時，遊戲從 60fps 掉到 30fps。

## 控制

遊戲執行中，可以用 `erh`（在 `cli/`）透過這次執行的輸出目錄跟模組溝通：

```sh
cargo build --release --manifest-path cli/Cargo.toml
cli/target/release/erh status        # 目前階段、玩家的 HP、所在區塊與位置
cli/target/release/erh ping
cli/target/release/erh nodead on     # HP 最低停在 1，不會死；`off` 取消
cli/target/release/erh warp 100001   # 用 BonfireWarpParam 的 row id 傳送到賜福
cli/target/release/erh flag ID off   # 讀取（不給狀態）或設定事件旗標，例如 boss 的擊敗旗標
cli/target/release/erh targethp 40   # 把鎖定中的敵人 HP 設成最大值的 40%
```

`erh` 會用 `$ERH_DIR`、`-d RUN_DIR`，都沒有的話用 `./out` 裡最新的一次。每個指令是一個檔案
`cmd/<id>.cmd`，模組在下一幀開頭、在遊戲的主執行緒上執行，結果寫進 `cmd/<id>.out`。
`status.tsv` 每 50ms 更新一次。

`warp` 呼叫的是遊戲自己的「傳送到賜福」。fromsoftware-rs 沒有這個函式的綁定，所以用位元組
樣式找到它；樣式和呼叫方式取自 [Erd-Tools](https://github.com/Nordgaren/Erd-Tools)。row id 可以查 Erd-Tools 的 `BonfireWarpParam.txt`，例如 100001 是
`[Stormveil Castle] Margit, the Fell Omen`（史東薇爾城的「惡兆妖鬼 瑪爾基特」賜福）。
