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
./launch.sh --run "scripts/boss.sh radahn-dlc" # 模組可以回應後，自動執行一個指令
```

`launch.sh` 會開 MangoHud（給 `MANGOHUD=0` 就不開）、設好 `ERH_OUT`，並且在這個目錄以外啟動 me3：me3 會優先從
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
cli/target/release/erh targethp 40   # 把目標的 HP 設成最大值的 40%：鎖定中的敵人，沒鎖定時是
                                     # 已載入角色中最大 HP 最高的那個
cli/target/release/erh nohit on      # 敵人的攻擊打不到玩家
cli/target/release/erh lockhp 50     # 按一次鎖定，鎖到的那一幀立刻設定敵人的 HP
cli/target/release/erh exterminate on # 玩家每一擊都造成 9999999 傷害
cli/target/release/erh flagdump alive # 把所有開著的事件旗標 id 寫到 flags-alive.txt
cli/target/release/erh graces       # 把所有賜福和它們的解鎖、清除旗標寫到 graces.tsv
cli/target/release/erh press skill 3 # 按住某個鍵 3 幀（預設 1 幀）
cli/target/release/erh probe attack  # 120 幀內，計算模組看到 attack 被按住幾幀；結果在 status
cli/target/release/erh scan          # 120 幀內，列出被按下的原始輸入編號；結果在 status
cli/target/release/erh rawpress 252 3 # 按住某個原始輸入編號，例如用 scan 找到的選單按鍵
cli/target/release/erh keypress 0x12 3 # 在鍵盤裝置裡按住某個 DirectInput 鍵碼（0x12 是 E）
cli/target/release/erh mousepress 0 3 # 在滑鼠裝置裡按住某個按鈕（0 是左鍵）
```

`erh` 會用 `$ERH_DIR`、`-d RUN_DIR`，都沒有的話用 `./out` 裡最新的一次。每個指令是一個檔案
`cmd/<id>.cmd`，模組在下一幀開頭、在遊戲的主執行緒上執行，結果寫進 `cmd/<id>.out`。
`status.tsv` 每 50ms 更新一次。

`warp` 呼叫的是遊戲自己的「傳送到賜福」。fromsoftware-rs 沒有這個函式的綁定，所以用位元組
樣式找到它；樣式和呼叫方式取自 [Erd-Tools](https://github.com/Nordgaren/Erd-Tools)。row id 可以查 Erd-Tools 的 `BonfireWarpParam.txt`，例如 100001 是
`[Stormveil Castle] Margit, the Fell Omen`（史東薇爾城的「惡兆妖鬼 瑪爾基特」賜福）。

`press` 可用的鍵：`attack`、`strong`、`guard`、`skill`、`roll`、`jump`、`item`、`lockon`、`action`。
按鍵寫進的是手把和鍵盤共用的虛擬輸入，所以會照玩家自己的按鍵設定。遊戲視窗有焦點時按鍵才有效，
所以 `erh` 要從不會搶走焦點的地方執行，例如從另一台裝置 ssh 進來。

對兩階段的 boss 用 `targethp 0`，會直接把它打死，不會進二階段；`targethp 1` 則什麼都不會發生
（瑪蓮妮亞實測如此）。換階段很可能是掛在「受到傷害」的流程上，而直接寫 HP 跳過了這個流程。
`exterminate on` 再打一下，是想改走受傷害的流程來結束一個階段，還沒實測。

實測：玩家鎖定 DLC 拉塔恩時，`targethp 50` 之後他就直接進二階段，不用再打；沒有鎖定就不會。他的抱摔技中兩次就會死，`nodead`
擋不住，這時要用 `nohit`。

`scripts/boss.sh` 把上面這些串起來，讓 `bosses.tsv` 裡打過的 boss 重新出現，也可以直接進後面的階段。
不帶參數會一步步問你；帶參數就直接執行（`--help` 列出選項，`--list` 列出 boss）：

```sh
scripts/boss.sh                        # 互動式
scripts/boss.sh radahn-dlc --nohit     # 二階段的 DLC 拉塔恩
scripts/boss.sh radahn-dlc --no-hp     # 一階段
```

它會關掉 boss 的旗標、傳送到圓桌廳堂再傳送回來（讓 boss 所在的地圖重新載入）、等 boss 載入，
再設定它的 HP；只在鎖定時才會換階段的 boss，會先鎖定。
