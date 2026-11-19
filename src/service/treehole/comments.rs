use super::Comment;
use std::{collections::HashMap, rc::Rc};

/// 保留服务端在同级的顺序，按父评论、各子分支的顺序生成可虚拟化的扁平列表。
pub(crate) fn comment_order(comments: &[Rc<Comment>]) -> Vec<(usize, usize)> {
    let ids: HashMap<_, _> = comments
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();
    let mut quoted = HashMap::new();
    let mut children = vec![Vec::new(); comments.len()];
    let mut roots = Vec::new();
    for (i, c) in comments.iter().enumerate() {
        let parent = c.reply_to.as_ref().and_then(|reply| {
            if let Some(id) = reply.id {
                return ids.get(&id).copied();
            }
            // 旧缓存没有父 ID，只接受前文中唯一的作者和完整原文组合。
            quoted
                .get(&(reply.username.as_deref()?, reply.content.as_deref()?))
                .copied()
                .flatten()
        });
        match parent {
            Some(p) if p != i => children[p].push(i),
            _ => roots.push(i),
        }
        if let (Some(name), Some(body)) = (c.username.as_deref(), c.content.as_deref())
            && !name.is_empty()
            && !body.is_empty()
        {
            quoted
                .entry((name, body))
                .and_modify(|p| *p = None)
                .or_insert(Some(i));
        }
    }

    let mut order = Vec::with_capacity(comments.len());
    let mut seen = vec![false; comments.len()];
    let mut stack = Vec::new();
    // 正常分支先展示；即便服务端返回循环引用，也让每条评论可见一次。
    for root in roots.into_iter().chain(0..comments.len()) {
        if seen[root] {
            continue;
        }
        stack.push((root, 0));
        while let Some((i, depth)) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            order.push((i, depth));
            stack.extend(children[i].iter().rev().map(|&child| (child, depth + 1)));
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn groups_siblings_and_cross_page_replies_without_guessing_missing_parents() {
        let comment = |id, body, reply: Value| {
            Rc::new(
                serde_json::from_value(json!({
                    "id":id, "username":"读者", "content":body, "reply_to":reply
                }))
                .unwrap(),
            )
        };
        let mut rows = vec![
            comment(10, "第一条", Value::Null),
            comment(20, "第二条", Value::Null),
            comment(30, "同级回复", json!({"id":10})),
            comment(40, "嵌套", json!({"id":30})),
            comment(50, "又一条", json!({"username":"读者","content":"第一条"})),
            comment(60, "父评论暂未到达", json!({"id":70})),
        ];
        assert_eq!(
            comment_order(&rows),
            [(0, 0), (2, 1), (3, 2), (4, 1), (1, 0), (5, 0)]
        );
        rows.push(comment(70, "第一条", Value::Null));
        rows.push(comment(
            80,
            "旧引用不唯一",
            json!({"username":"读者","content":"第一条"}),
        ));
        rows.push(comment(
            90,
            "目标已删除",
            json!({"id":999,"username":"读者","content":"第二条"}),
        ));
        assert_eq!(
            comment_order(&rows),
            [
                (0, 0),
                (2, 1),
                (3, 2),
                (4, 1),
                (1, 0),
                (6, 0),
                (5, 1),
                (7, 0),
                (8, 0)
            ]
        );
    }
}
