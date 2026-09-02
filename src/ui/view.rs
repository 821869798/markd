use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::app::{App, Pane};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewLayout {
    pub search: Rect,
    pub categories: Rect,
    pub bookmarks: Rect,
}

impl ViewLayout {
    pub fn category_content(self) -> Rect {
        bordered_inner(self.categories)
    }

    pub fn bookmark_content(self) -> Rect {
        bordered_inner(self.bookmarks)
    }
}

pub(crate) fn layout_for(area: Rect) -> ViewLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(vertical[1]);
    ViewLayout {
        search: vertical[0],
        categories: body[0],
        bookmarks: body[1],
    }
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);
    let layout = layout_for(area);
    let category_rows = usize::from(layout.category_content().height);
    let bookmark_rows = usize::from(layout.bookmark_content().height);
    app.set_viewport_rows(category_rows, bookmark_rows);

    let search_mode = if app.is_searching() {
        " [输入中]"
    } else {
        ""
    };
    let search = if app.search_text().is_empty() {
        "/".to_owned()
    } else {
        app.search_text().to_owned()
    };
    frame.render_widget(
        Paragraph::new(Line::from(format!("mkd  搜索: {search}{search_mode}")))
            .block(Block::default().borders(Borders::ALL).title("状态")),
        vertical[0],
    );

    render_categories(frame, app, layout);
    render_bookmarks(frame, app, layout);

    let footer = match app.status_message() {
        Some(message) => format!("错误: {message}"),
        None if app.is_searching() => {
            "搜索中: 输入文字过滤  Enter 跳转  Esc 退出搜索（管理操作需先退搜索）".to_owned()
        }
        None => "↑/↓ 或 j/k 移动  Tab 切栏  / 搜索  a 归组  c 新建分组  y 复制  h 帮助  Enter 跳转"
            .to_owned(),
    };
    let footer_style = if app.status_message().is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(footer).style(footer_style), vertical[2]);

    if app.is_confirming_delete() {
        let popup = centered_rect(60, 20, area);
        frame.render_widget(
            Paragraph::new("确认删除当前书签？再次按 d 确认，Esc 取消")
                .block(Block::default().borders(Borders::ALL).title("确认")),
            popup,
        );
    } else if app.is_editing() {
        render_edit_popup(frame, area, app);
    } else if app.is_help_visible() {
        render_help_popup(frame, area);
    }
}

fn render_edit_popup(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.is_picking_category() {
        render_category_picker(frame, area, app);
        return;
    }
    // Fixed 5-row popup: 2 borders + input + blank + hint. Percentages make
    // short terminals truncate the hint, which confused users.
    let popup = Rect {
        x: area.x + area.width.saturating_sub(50) / 2,
        y: area.y + area.height.saturating_sub(5) / 2,
        width: area.width.min(50),
        height: area.height.min(5),
    };
    frame.render_widget(Clear, popup);
    let title = app.edit_prompt().unwrap_or("编辑");
    let hint = match title {
        "新建分类" => "输入新分组名，回车提交，Esc 取消",
        "重命名分类" => "输入新名称，回车提交，Esc 取消",
        _ => "输入新名称，回车提交，Esc 取消",
    };

    let input_line = format!("> {}", app.edit_text());
    let lines: Vec<Line> = vec![Line::from(input_line), Line::from(""), Line::from(hint)];
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        popup,
    );
    // Place the cursor right after the typed text so input is visible.
    let input_x = popup
        .x
        .saturating_add(2 + app.edit_text().chars().count() as u16)
        .min(popup.right().saturating_sub(2));
    frame.set_cursor_position((input_x, popup.y + 1));
}

fn render_category_picker(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.picker_items();
    let rows = items.len() as u16 + 2; // borders
    let height = rows.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(40) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: area.width.min(40),
        height,
    };
    frame.render_widget(Clear, popup);
    let selected = app.picker_index();
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let style = if index == selected {
                Style::default().bg(Color::White).fg(Color::Black)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(format!("  {item}"))).style(style)
        })
        .collect();
    frame.render_widget(
        List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("选择分组 (↑↓ 选，回车确认，n 新建)"),
        ),
        popup,
    );
}

fn render_help_popup(frame: &mut Frame<'_>, area: Rect) {
    let popup = Rect {
        x: area.x + area.width.saturating_sub(52) / 2,
        y: area.y + area.height.saturating_sub(26) / 2,
        width: area.width.min(52),
        height: area.height.min(26),
    };
    frame.render_widget(Clear, popup);
    let help = vec![
        "mkd — 目录书签 快捷键",
        "",
        "导航",
        "  j / k / ↑ / ↓       上下移动",
        "  Alt+↑ / Alt+↓       移动书签顺序（手动排序区）",
        "  Tab                  左右栏切换",
        "  /                    进入搜索（再按 Esc 退出搜索）",
        "  鼠标单击             选择分类或书签 / 点搜索框进入搜索",
        "  鼠标双击             直接跳转到书签目录",
        "",
        "书签",
        "  Enter                跳转到选中目录",
        "  y                    复制选中书签路径到剪贴板",
        "  a                    归组：为选中书签选择分组（弹窗上下选）",
        "  e                    重命名书签",
        "  d → d                删除书签（按两次确认）",
        "",
        "分组（分类）",
        "  c                    新建分组（输入名字）",
        "  r                    重命名分组（选中分组后）",
        "  D → D                删除分组（书签归回 default）",
        "",
        "其他",
        "  h                    显示/关闭此帮助",
        "  Esc                  取消当前操作 / 退出",
    ];
    let help_lines: Vec<Line> = help.into_iter().map(Line::from).collect();
    frame.render_widget(
        Paragraph::new(help_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("帮助 (h 关闭)"),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_categories(frame: &mut Frame<'_>, app: &App, layout: ViewLayout) {
    let rows = usize::from(layout.category_content().height);
    let offset = app.category_offset();
    let items = app
        .categories()
        .iter()
        .skip(offset)
        .take(rows)
        .map(|category| ListItem::new(category.as_str()))
        .collect::<Vec<_>>();
    let border_style = pane_border_style(app.active_pane() == Pane::Categories);
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("分类")
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if rows > 0
        && app.selected_category() >= offset
        && app.selected_category() < offset.saturating_add(rows)
    {
        state.select(Some(app.selected_category() - offset));
    }
    frame.render_stateful_widget(list, layout.categories, &mut state);
}

fn render_bookmarks(frame: &mut Frame<'_>, app: &App, layout: ViewLayout) {
    let rows = usize::from(layout.bookmark_content().height);
    let offset = app.bookmark_offset();
    let border_style = pane_border_style(app.active_pane() == Pane::Bookmarks);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("书签")
        .border_style(border_style);

    if app.visible_bookmarks().is_empty() {
        let message = if app.search_text().is_empty() {
            "还没有书签"
        } else {
            "没有匹配结果"
        };
        frame.render_widget(Paragraph::new(message).block(block), layout.bookmarks);
        return;
    }

    let items = app
        .visible_bookmarks()
        .iter()
        .skip(offset)
        .take(rows)
        .map(|bookmark| {
            let invalid = if bookmark.valid_target {
                String::new()
            } else {
                " [失效]".to_owned()
            };
            ListItem::new(format!(
                "{}{} — {}",
                bookmark.name,
                invalid,
                bookmark.path.display()
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if let Some(selected) = app.selected_bookmark()
        && rows > 0
        && selected >= offset
        && selected < offset.saturating_add(rows)
    {
        state.select(Some(selected - offset));
    }
    frame.render_stateful_widget(list, layout.bookmarks, &mut state);
}

fn pane_border_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn bordered_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::model::{Bookmark, Database};
    use crate::ui::app::{Action, App, Mutation};
    use chrono::{TimeZone, Utc};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn category_picker_moves_bookmark_and_supports_create_new() {
        let mut database = Database::default();
        database.add_category("work").unwrap();
        let temp = tempfile::tempdir().unwrap();
        database
            .add_bookmark(temp.path().to_path_buf(), Some("repo".into()), None)
            .unwrap();
        let mut app = App::from_database(database);

        // Open the picker on the first bookmark.
        app.handle(Action::Down, Utc::now());
        app.handle(Action::BeginAddBookmark, Utc::now());
        assert!(app.is_picking_category());

        // Down to "新建分组…", then create-new instead of selecting.
        app.handle(Action::BrowseCategoriesDown, Utc::now());
        app.handle(Action::BrowseCategoriesSelect, Utc::now());
        assert_eq!(app.edit_prompt(), Some("新建分类"));

        // Cancel out; reopen and pick "work" directly this time.
        app.handle(Action::Cancel, Utc::now());
        app.handle(Action::BeginAddBookmark, Utc::now());
        app.handle(Action::BrowseCategoriesSelect, Utc::now());
        let Some(Mutation::MoveBookmark { new_category, .. }) = app.take_mutation() else {
            panic!("expected MoveBookmark mutation");
        };
        assert_eq!(new_category, "work");
    }

    #[test]
    fn picker_navigation_is_bounded_and_requires_a_selection() {
        let mut database = Database::default();
        database.add_category("work").unwrap();
        let mut app = App::from_database(database);

        // No bookmark selected: `a` is a no-op with a hint.
        app.handle(Action::BeginAddBookmark, Utc::now());
        assert!(!app.is_picking_category());
        assert_eq!(
            app.status_message(),
            Some("先选中一个书签（右侧列表）再归组")
        );

        // With a selection the picker opens and clamps navigation.
        let temp = tempfile::tempdir().unwrap();
        let mut db2 = Database::default();
        db2.add_category("work").unwrap();
        db2.add_bookmark(temp.path().to_path_buf(), Some("repo".into()), None)
            .unwrap();
        let mut app2 = App::from_database(db2);
        app2.handle(Action::Down, Utc::now());
        app2.handle(Action::BeginAddBookmark, Utc::now());
        for _ in 0..10 {
            app2.handle(Action::BrowseCategoriesDown, Utc::now());
        }
        assert_eq!(app2.picker_index(), 1); // items: [work, 新建分组…]
        for _ in 0..10 {
            app2.handle(Action::BrowseCategoriesUp, Utc::now());
        }
        assert_eq!(app2.picker_index(), 0);
    }

    #[test]
    fn empty_database_renders_empty_state() {
        let mut app = App::from_database_at(Database::default(), test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("还没有书签"), "{buffer}");
    }

    #[test]
    fn help_overlay_opens_and_closes_and_shows_shortcuts() {
        let mut app = App::from_database_at(Database::default(), test_now());
        app.handle(Action::ToggleHelp, test_now());
        let buffer = render_to_string(&mut app, 100, 30);
        assert!(buffer.contains("快捷键"), "{buffer}");
        assert!(buffer.contains("复制选中书签路径"), "{buffer}");
        app.handle(Action::ToggleHelp, test_now());
        let closed = render_to_string(&mut app, 100, 30);
        assert!(!closed.contains("快捷键"), "{closed}");
    }

    #[test]
    fn help_overlay_esc_closes_and_other_actions_are_locked() {
        let mut app = App::from_database_at(Database::default(), test_now());
        app.handle(Action::ToggleHelp, test_now());
        // While help is open, other actions must not leak through.
        app.handle(Action::StartSearch, test_now());
        assert!(!app.is_searching());
        app.handle(Action::Cancel, test_now());
        assert!(!app.is_help_visible());
    }

    #[test]
    fn edit_popup_shows_input_marker_and_hint() {
        let mut app = App::from_database_at(Database::default(), test_now());
        app.handle(Action::BeginCreateCategory, test_now());
        app.handle(Action::Input('w'), test_now());
        let buffer = render_to_string(&mut app, 80, 24);
        assert!(buffer.contains(">w"), "{buffer}");
        assert!(buffer.contains("输入新分组名"), "{buffer}");
    }

    #[test]
    fn search_without_matches_renders_no_match_state() {
        let database = database_with_bookmark("alpha", PathBuf::from("missing-alpha"));
        let mut app = App::from_database_at(database, test_now());
        app.handle(Action::StartSearch, test_now());
        for character in "no-such-bookmark-987654321".chars() {
            app.handle(Action::Input(character), test_now());
        }

        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("没有匹配结果"), "{buffer}");
    }

    #[test]
    fn missing_path_is_visibly_marked_invalid() {
        let database = database_with_bookmark("missing", PathBuf::from("missing-directory"));
        let mut app = App::from_database_at(database, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("失效"), "{buffer}");
        assert!(buffer.contains("missing"), "{buffer}");
    }

    #[test]
    fn regular_file_is_visibly_marked_invalid() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let database = database_with_bookmark("regular-file", file.path().to_path_buf());
        let mut app = App::from_database_at(database, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("失效"), "{buffer}");
        assert!(buffer.contains("regular-file"), "{buffer}");
    }

    #[test]
    fn status_message_is_rendered_in_the_footer() {
        let database = database_with_bookmark("missing", PathBuf::from("missing-directory"));
        let mut app = App::from_database_at(database, test_now());
        app.handle(Action::Confirm, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("目录不存在或不是目录"), "{buffer}");
    }

    #[test]
    fn narrow_and_short_terminals_do_not_panic() {
        for (width, height) in [(1, 1), (4, 2), (10, 3)] {
            let mut app = App::from_database_at(Database::default(), test_now());
            let _ = render_to_string(&mut app, width, height);
        }
    }

    fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .replace(' ', "")
    }

    fn database_with_bookmark(name: &str, path: PathBuf) -> Database {
        Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![Bookmark {
                id: Uuid::new_v4(),
                name: name.into(),
                path,
                category: "default".into(),
                created_at: test_now(),
                last_visited_at: None,
                visit_count: 0,
                sort_key: None,
            }],
        }
    }

    fn test_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }
}
