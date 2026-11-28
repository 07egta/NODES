//! Turns an engine error message into three short Japanese lines:
//! what happened, why, and how to fix it. Pure text; no UI here.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Friendly {
    /// 何が起きたか。
    pub what: String,
    /// なぜ起きたか。
    pub why: &'static str,
    /// どう直すか。
    pub fix: &'static str,
}

/// First sentence of a message (up to and including the first 「。」), at most 120 chars.
fn first_sentence(message: &str) -> String {
    let end = message
        .find('。')
        .map_or(message.len(), |i| i + '。'.len_utf8());
    message[..end].trim().chars().take(120).collect()
}

/// Explains `message` (as returned by the engine) in plain Japanese.
pub fn explain(message: &str) -> Friendly {
    let (why, fix): (&'static str, &'static str) = if message.contains("接続してください") {
        (
            "このノードは、材料になるデータを左の入力から受け取る必要があります。",
            "足りない入力の丸へ、前のノードの出力（右の丸）から線をドラッグしてつなぎます。",
        )
    } else if message.contains("ゼロでは割れません") || message.contains("0 以外") {
        (
            "割り算の分母が 0 になっています。",
            "分母（B）を 0 以外の数にしてください。",
        )
    } else if message.contains("大きすぎ") || message.contains("上限") || message.contains("多すぎ")
    {
        (
            "個数や分割数が多く、パソコンが安全に計算できる量を超えました。",
            "個数・分割数・層の数を半分くらいに減らしてみてください。",
        )
    } else if message.contains("自己交差") || message.contains("重複する点") {
        (
            "輪郭の線がねじれて交わっているか、同じ点が重なっています。",
            "輪郭の点の位置や半径を変えて、線が交わらない形にしてください。",
        )
    } else if message.contains("以上にしてください") || message.contains("より小さく")
    {
        (
            "設定した数値が、このノードの許す範囲から外れています。",
            "ノードを選び、右の設定で数値を範囲内に戻してください。範囲は F1 の辞典で確認できます。",
        )
    } else if message.contains("有限") || message.contains("無効") {
        (
            "計算の途中で、数でない値（0 で割る・大きすぎる値など）が出ました。",
            "上流のノードの数値を確認し、0 や極端に大きい値を避けてください。「値を見る」で中身を確認できます。",
        )
    } else if message.contains("停止") {
        (
            "計算が途中で止められました。",
            "数値を変えると自動でもう一度計算します。",
        )
    } else if message.contains("平面") || message.contains("XY") || message.contains("XZ") {
        (
            "入力された形の向きや位置が、このノードの想定と合っていません。",
            "移動・回転ノードで形の向きを合わせてください。",
        )
    } else {
        (
            "入力の値や形の組み合わせが、このノードの条件に合っていません。",
            "ノードを選んで F1 で説明を読み、上流のノードの値を見直してください。",
        )
    };
    Friendly {
        what: first_sentence(message),
        why,
        fix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_points_to_connecting_a_wire() {
        let f = explain("曲線を接続してください。");
        assert_eq!(f.what, "曲線を接続してください。");
        assert!(f.fix.contains("線"));
    }

    #[test]
    fn divide_by_zero_and_size_limits_are_recognised() {
        assert!(
            explain("ゼロでは割れません。B を 0 以外にしてください。")
                .why
                .contains("分母")
        );
        assert!(
            explain("形状が大きすぎます。分割数を減らしてください。")
                .fix
                .contains("減らして")
        );
    }

    #[test]
    fn unknown_messages_still_get_three_lines() {
        let f = explain("なにかのエラー");
        assert_eq!(f.what, "なにかのエラー");
        assert!(!f.why.is_empty() && !f.fix.is_empty());
    }

    #[test]
    fn long_messages_are_cut_at_the_first_sentence() {
        let f = explain("一つめ。二つめ。");
        assert_eq!(f.what, "一つめ。");
        assert!(explain(&"あ".repeat(500)).what.chars().count() <= 120);
    }
}
