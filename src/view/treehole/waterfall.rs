use super::card::Card;
use crate::service::treehole::Post;
use gtk::{Fixed, Orientation::Vertical, ScrolledWindow, prelude::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

const GAP: i32 = 12;
const EDGE: i32 = 16;

struct Slot {
    index: usize,
    y: i32,
    height: i32,
}

/// 分列保存位置，滚动时只查找每列与视口相交的区段。
pub(super) struct Waterfall {
    pub scroll: ScrolledWindow,
    canvas: Fixed,
    probe: Card,
    like_width: gtk::SizeGroup,
    comment_width: gtk::SizeGroup,
    likes: Rc<super::likes::Likes>,
    columns: Vec<Vec<Slot>>,
    width: i32,
    font: Option<gtk::pango::FontDescription>,
    measured: usize,
    active: BTreeMap<usize, Card>,
    pool: Vec<Card>,
    open: Rc<dyn Fn(usize)>,
}

impl Waterfall {
    pub fn new(open: Rc<dyn Fn(usize)>, footer: &gtk::Box, likes: Rc<super::likes::Likes>) -> Self {
        let canvas = Fixed::new();
        let content = gtk::Box::new(Vertical, 0);
        content.append(&canvas);
        content.append(footer);
        // 键盘导航自行定位；GTK 的自动焦点滚动会在详情返回时再次移动列表。
        let viewport = gtk::Viewport::builder()
            .scroll_to_focus(false)
            .child(&content)
            .build();
        let scroll = ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::External)
            .child(&viewport)
            .build();
        let probe = Card::new(Rc::new(|_| {}));
        let measuring = gtk::Box::new(Vertical, 0);
        measuring.append(&probe.tile);
        measuring.set_visible(false);
        canvas.put(&measuring, 0., 0.);
        Self {
            scroll,
            canvas,
            probe,
            like_width: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            comment_width: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
            likes,
            width: 0,
            font: None,
            measured: 0,
            columns: vec![],
            active: BTreeMap::new(),
            pool: vec![],
            open,
        }
    }

    pub fn update_card(&self, index: usize, post: &Post) {
        if let Some(card) = self.active.get(&index) {
            card.bind(index, post, Some(&self.likes));
        }
    }

    pub fn invalidate(&mut self) {
        self.width = 0;
        for (_, card) in std::mem::take(&mut self.active) {
            self.canvas.remove(&card.tile);
            self.pool.push(card);
        }
    }

    pub fn near_end(&self) -> bool {
        // 追加后 GTK 尚未更新 adjustment.upper；使用刚测出的内容高度。
        let adj = self.scroll.vadjustment();
        self.width > 0
            && adj.page_size() > 0.
            && adj.value() + adj.page_size() + 180. >= f64::from(self.canvas.height_request())
    }

    /// 卡片会被复用，Tab 顺序按帖子顺序推进，并滚动到新焦点。
    pub fn focus_relative(&mut self, posts: &[Post], offset: isize) -> bool {
        let Some((&current, card)) = self
            .active
            .iter()
            .find(|(_, card)| card.button.has_focus() || card.like.button.has_focus())
        else {
            return false;
        };
        if offset > 0 && card.button.has_focus() && card.like.button.is_sensitive() {
            return card.like.button.grab_focus();
        }
        if offset < 0 && card.like.button.has_focus() {
            return card.button.grab_focus();
        }
        let Some(next) = current
            .checked_add_signed(offset)
            .filter(|i| *i < posts.len())
        else {
            return false;
        };
        let Some(target) = self.columns.iter().find_map(|col| {
            col.binary_search_by_key(&next, |s| s.index)
                .ok()
                .map(|i| &col[i])
        }) else {
            return false;
        };
        self.scroll
            .vadjustment()
            .clamp_page(target.y.into(), (target.y + target.height).into());
        self.render(posts);
        self.active.get(&next).is_some_and(|card| {
            if offset < 0 && card.like.button.is_sensitive() {
                card.like.button.grab_focus()
            } else {
                card.button.grab_focus()
            }
        })
    }

    pub fn render(&mut self, posts: &[Post]) {
        let width = self.scroll.hadjustment().page_size().round() as i32;
        if width <= 2 * EDGE {
            return;
        }
        let count = if width >= 980 {
            3
        } else if width >= 640 {
            2
        } else {
            1
        };
        let card_width = (width - 2 * EDGE - GAP * (count - 1)) / count;
        let font = self.scroll.pango_context().font_description();
        let resized = width != self.width || font != self.font;
        if resized {
            self.width = width;
            self.font = font;
            self.columns = (0..count).map(|_| Vec::new()).collect();
            self.measured = 0;
        }
        if resized || self.measured != posts.len() {
            self.probe.tile.set_size_request(card_width, -1);
            for (index, post) in posts.iter().enumerate().skip(self.measured) {
                self.probe.bind(index, post, None);
                let height = self.probe.tile.measure(Vertical, card_width).1.max(1);
                let column = self.columns.iter_mut().min_by_key(|c| bottom(c)).unwrap();
                column.push(Slot {
                    index,
                    y: bottom(column),
                    height,
                });
            }
            self.measured = posts.len();
            let height = self.columns.iter().map(|c| bottom(c)).max().unwrap_or(EDGE);
            self.canvas.set_size_request(1, height + EDGE);
        }
        let adj = self.scroll.vadjustment();
        let top = adj.value() as i32 - 240;
        let end = (adj.value() + adj.page_size()) as i32 + 240;
        let mut wanted = BTreeSet::new();
        let mut slots = Vec::new();
        for (c, col) in self.columns.iter().enumerate() {
            let start = col.partition_point(|s| s.y + s.height < top);
            for slot in col[start..].iter().take_while(|s| s.y <= end) {
                wanted.insert(slot.index);
                slots.push((
                    slot.index,
                    EDGE + c as i32 * (card_width + GAP),
                    slot.y,
                    slot.height,
                ));
            }
        }
        let stale: Vec<_> = self
            .active
            .keys()
            .filter(|i| !wanted.contains(i))
            .copied()
            .collect();
        for i in stale {
            let card = self.active.remove(&i).unwrap();
            self.canvas.remove(&card.tile);
            self.pool.push(card);
        }
        for (i, x, y, h) in slots {
            if let Some(card) = self.active.get(&i) {
                if resized {
                    card.tile.set_size_request(card_width, h);
                    self.canvas.move_(&card.tile, x.into(), y.into());
                }
            } else {
                let card = self.pool.pop().unwrap_or_else(|| {
                    let card = Card::new(self.open.clone());
                    self.like_width.add_widget(&card.like.number);
                    self.comment_width.add_widget(&card.counts);
                    card
                });
                card.bind(i, &posts[i], Some(&self.likes));
                card.tile.set_size_request(card_width, h);
                self.canvas.put(&card.tile, x.into(), y.into());
                self.active.insert(i, card);
            }
        }
    }
}

fn bottom(slots: &[Slot]) -> i32 {
    slots.last().map_or(EDGE, |s| s.y + s.height + GAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib;
    use std::time::{Duration, Instant};

    fn settle() {
        let until = Instant::now() + Duration::from_millis(120);
        while Instant::now() < until {
            glib::MainContext::default().iteration(false);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    #[ignore = "需要 GTK 显示后端"]
    fn waterfall_recycles_wraps_and_opens_details_at_the_same_position() {
        gtk::test_synced(|| {
            adw::init().unwrap();
            crate::view::install_styles();
            let posts: Rc<Vec<Post>> = Rc::new(
                (0..1500)
                    .map(|i| Post {
                        id: i,
                        title: Some("窗边的日记".into()),
                        content: Some("冬天的阳光落在窗边。\r\n".repeat(if i % 3 == 0 {
                            90
                        } else {
                            1
                        })),
                        username: Some("读者".into()),
                        createdtime: None,
                        like_count: Some(if i == 1 { 1648 } else { 3 }),
                        comment_count: Some(if i == 1 { 5200 } else { 0 }),
                        liked: None,
                    })
                    .collect(),
            );
            let nav = adw::NavigationView::new();
            let flow_nav = nav.downgrade();
            let entries = posts.clone();
            let mut flow = Waterfall::new(
                Rc::new(move |i| {
                    flow_nav
                        .upgrade()
                        .unwrap()
                        .push(&crate::view::treehole::card::detail(
                            &entries[i],
                            crate::view::treehole::likes::mock(),
                            || Err("测试"),
                            |_, _| Ok(vec![]),
                            |_, _| Ok(()),
                            || {},
                        ))
                }),
                &gtk::Box::new(Vertical, 0),
                crate::view::treehole::likes::mock(),
            );
            nav.add(&adw::NavigationPage::new(&flow.scroll, "树洞"));
            let win = gtk::Window::builder()
                .default_width(800)
                .default_height(560)
                .child(&nav)
                .build();
            win.present();
            settle();
            flow.render(&posts[..1]);
            assert!(flow.near_end(), "不足一屏时应继续翻页");
            flow.render(&posts[..1000]);
            assert!(!flow.near_end(), "追加后应使用新内容高度");
            settle();
            assert_eq!(flow.columns.len(), 2);
            let card = &flow.active[&1];
            assert_eq!(card.like.number.text(), "1648");
            let number = &card.counts;
            assert_eq!(number.text(), "5200");
            assert_eq!(number.layout().line_count(), 1);
            let bounds = number.compute_bounds(&card.tile).unwrap();
            assert!(bounds.x() + bounds.width() <= card.tile.allocated_width() as f32);
            let positions = |card: &Card| {
                let mut positions = Vec::new();
                for number in [&card.like.number, &card.counts] {
                    let bounds = number.compute_bounds(&card.tile).unwrap();
                    let icon = number
                        .prev_sibling()
                        .unwrap()
                        .compute_bounds(&card.tile)
                        .unwrap();
                    assert_eq!((icon.width(), icon.height()), (16., 16.));
                    assert_eq!(bounds.x() - icon.x() - icon.width(), 4.);
                    let baseline = bounds.y()
                        + number.layout_offsets().1 as f32
                        + number.layout().baseline() as f32 / gtk::pango::SCALE as f32;
                    positions.push((icon.x(), icon.y(), baseline));
                }
                assert!((positions[0].1 - positions[1].1).abs() <= 1.);
                assert!((positions[0].2 - positions[1].2).abs() <= 1.);
                [positions[0].0, positions[1].0]
            };
            assert_eq!(positions(card), positions(&flow.active[&0]));
            assert!(flow.columns[0][0].height > flow.columns[1][0].height);
            let preview = flow.active[&0]
                .button
                .child()
                .unwrap()
                .last_child()
                .and_downcast::<gtk::Label>()
                .unwrap();
            assert!(preview.layout().is_ellipsized() || preview.text().ends_with('…'));
            assert_eq!(preview.layout().line_count(), 6);
            let adj = flow.scroll.vadjustment();
            adj.set_value(18_000.);
            flow.render(&posts[..1000]);
            settle();
            let index = *flow.active.keys().nth(2).unwrap();
            assert!(index > 20);

            assert!(flow.active[&index].button.grab_focus());
            assert!(flow.focus_relative(&posts[..1000], 1));
            assert!(flow.active[&index].like.button.has_focus());
            assert!(flow.focus_relative(&posts[..1000], 1));
            assert!(flow.active[&(index + 1)].button.has_focus());
            let value = adj.value();
            flow.active[&(index + 1)].button.emit_clicked();
            settle();
            assert_eq!(nav.navigation_stack().n_items(), 2);
            assert!(nav.pop());
            settle();
            flow.render(&posts);
            settle();
            assert!((adj.value() - value).abs() < 1.);
            adj.set_value(adj.upper() - adj.page_size());
            flow.render(&posts);
            settle();
            assert!(flow.near_end());
            assert!(flow.active.contains_key(&(posts.len() - 1)));
            assert!(flow.pool.len() + flow.active.len() < 60);
            win.set_default_size(460, 560);
            settle();
            flow.render(&posts);
            settle();
            assert_eq!(flow.columns.len(), 1);
            win.close();
        });
    }
}
