# Native diagnostic recovery check

Executed on 2026-11-27 JST in the Linux/Xfce desktop, using the compact-debug
NODES binary from this workstream. This is a manual application check, separate
from unit tests and CI. It is not a Windows/macOS or geometric-output test.

## Reproducible input

Open [`tests/fixtures/diagnostics/two-roots.morph`](../../tests/fixtures/diagnostics/two-roots.morph)
in NODES. This fixture intentionally contains runtime errors and therefore lives
outside the successful `examples` collection. It is structurally valid and has
six nodes:

| ID | Node | Initial result |
| --- | --- | --- |
| 0 | Divide, 半径の計算, A=12 / B=0 | Independent division error |
| 1 | Divide, 高さの計算, A=20 / B=0 | Independent division error |
| 2 | Add, 寸法を合計, inputs 0 and 1 | Blocked by either upstream root |
| 3 | Multiply, 仕上げ係数, input 2 multiplied by 2 | Blocked downstream |
| 4 | Number, 別系統の正常値, value=10 | Successful independent branch |
| 5 | Disabled Divide, 無効にした試作 | Disabled; not a root failure |

The real console command `nodes-cli diagnose <fixture>` was also observed to
return exit code 1 with schema 1, two roots, two blocked nodes, four failed nodes,
one successful node and one disabled node. The diagnostic operation leaves the
source document unchanged.

## Observed workflow

1. Launched the native application and confirmed all six nodes and runtime
   errors rendered. Opened the status-bar **診断** button.
2. The **入力とエラー** view displayed two root candidates. Each root reported two
   downstream failures; the blocked section counted two nodes rather than four.
   The disabled section counted one.
3. Clicked the `#00 半径の計算` link. The canvas fitted/selected that node and the
   inspector showed the correct Divide node, ID and original division error.
4. Changed B from 0 to 35 using its inspector slider. The diagnostic view updated
   to one remaining root (`#01 高さの計算`) and the status bar to three failures.
5. Clicked that remaining root and changed its B from 0 to 35. The diagnostic
   window changed to **計算エラーはありません**. The disabled node remained listed
   and the worker reported three computed plus two reused enabled nodes.
6. Opened **処理時間**. It showed three computed nodes. Enabling the cache checkbox
   showed all five enabled nodes, with the additional two labeled **再利用**.
   The disabled node did not become a measured active node.

Each stage was visually inspected through native screenshots. Screenshot files
are held by the desktop session and are not included in this repository. The
fixture and these exact steps make the check reproducible after that session ends.

## Limits and remaining checks

- The name/ID filter, 101st-row paging and live-renaming behavior have automated
  logic tests. They were not typed through during this first GUI check because bound
  keyboard input did not reach the native numeric field in this environment.
  Pointer/sliders worked after moving the pointer before clicking. This is an
  automation limitation; it does not establish an application keyboard defect.
- No keyboard-based numeric entry, clipboard, screen-reader or touch
  accessibility claim is made from this pointer-driven check.
- Timing values on this small graph are transient measurements, not a benchmark
  or a performance guarantee.
- Geometry preview, printability and fabrication remain separate verification
  tasks. This fixture contains only numeric values.

## Precise numeric values and actual clipboard transfer

Executed a second native check on 2026-11-27 JST with the debug desktop binary
built from `147148edb7d1fe70a0cb9096b9fec4578e020ab6`.
The saved [`numeric-details.morph`](../../tests/fixtures/diagnostics/numeric-details.morph)
passed real CLI safe validation and evaluation: all six nodes succeeded.

1. Launched that file, selected **ほぼ平行な角度**, opened **値を見る** and
   **別ウィンドウで開く**. The output displayed **5.0000e-19**, rather than zero.
2. Hovered the output and observed the detailed value **5.00000001e-19**.
3. Right-clicked it, selected **詳しい値をコピー**, launched a blank Mousepad
   document and used the operating system's Paste shortcut. Both the screenshot
   and accessibility text contained exactly **5.00000001e-19**. No string was
   typed into the destination to simulate this transfer.
4. Selected **最小の正数**. The viewer displayed **4.9407e-324**, preserving a
   nonzero preview of the least positive subnormal.
5. Selected **符号を持つゼロ**. The viewer displayed **-0.0000**.
6. Closed the temporary unsaved clipboard test document and stopped only this
   owned test application. The concurrent developer's other NODES window remained.

This verifies an actual Linux scalar clipboard path. Coordinate copying, keyboard
parameter editing, large-number GUI display and Windows clipboard behavior were
not checked in this second workflow; formatting tests cover finite scalar/XYZ
round trips and range endpoints separately.

Two additional observations were retained instead of treating screenshots as an
unqualified UI pass:

- The viewer footer still described all values as four decimal places. The
  follow-up changes that text to describe abbreviated previews and the detail
  affordances. No numerical behavior changes with this wording correction.
- The existing parameter slider's full decimal text for the least subnormal
  widened the inspector and narrowed the canvas. The new result viewer itself
  remained compact. Parameter editing is owned by concurrent PR #16; reproduce
  this case there when integrating its signed-zero editor. This branch does not
  claim to have fixed that separate input layout issue.

## Paging, node navigation and filtering

A third native workflow used the debug binary from `f858186` and
[`paging.morph`](../../tests/fixtures/diagnostics/paging.morph). Actual release CLI
diagnosis first confirmed 101 independent roots, zero blocked nodes and one
disabled node; its exit code was 1 and the fixture checksum was unchanged.

1. Opened **診断** and observed 101 causes with **1–100件目** on the first page.
2. Clicked **次の100件**. The page showed **101–101件目**, the sole entry
   **#100 最後の原因**, and the separate disabled-node section.
3. Clicked that entry and observed the selected/framed node and inspector ID
   **#100** with its original division error.
4. Focused the filter and entered **#100** through native printable-key input.
   The root section showed **該当1件 / 全101件** and the expected node.
5. Replaced the query with **001**. Name-fragment filtering showed exactly
   **#01 原因001**. The disabled section showed zero matching out of one total.

Japanese key injection was attempted but the desktop provider rejected the
keysym before entering Japanese characters. This does not establish a Japanese
input defect in NODES; Japanese normalization remains covered by the logic tests.
Live renaming and keyboard parameter editing were not part of this workflow.

This check discovered a real UI defect: a nonempty query made the resizable
diagnostic window grow horizontally on successive frames. The code reserved a
fixed width for the clear button without including TextEdit's outer margins and
the actual button/spacing width. The follow-up lays out the button first at the
right, then lets the text field consume the remaining width. A real-theme,
60-frame headless egui regression failed on the old layout and passed after this
change. All five diagnostic UI tests passed. Native confirmation of the fixed
window width is recorded separately when performed.
