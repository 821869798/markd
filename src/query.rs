use crate::model::{Bookmark, Database};
use chrono::{DateTime, Duration, Utc};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

/// Filters applied before ranking bookmarks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Query<'a> {
    pub category: Option<&'a str>,
    pub search: &'a str,
}

/// A bookmark together with the values used to rank it.
#[derive(Debug)]
pub struct QueryResult<'a> {
    pub bookmark: &'a Bookmark,
    pub fuzzy_score: i64,
    pub access_score: i64,
    pub path_exists: bool,
}

/// Filters and deterministically ranks bookmarks for display.
pub fn query_bookmarks<'a>(
    db: &'a Database,
    query: Query<'_>,
    now: DateTime<Utc>,
) -> Vec<QueryResult<'a>> {
    let matcher = SkimMatcherV2::default().ignore_case();
    let mut results = db
        .bookmarks
        .iter()
        .filter(|bookmark| {
            query
                .category
                .is_none_or(|category| bookmark.category == category)
        })
        .filter_map(|bookmark| {
            let fuzzy_score = if query.search.is_empty() {
                0
            } else {
                let name_score = matcher.fuzzy_match(&bookmark.name, query.search);
                let path_score =
                    matcher.fuzzy_match(&bookmark.path.to_string_lossy(), query.search);
                name_score.into_iter().chain(path_score).max()?
            };

            Some(QueryResult {
                bookmark,
                fuzzy_score,
                access_score: access_score(bookmark, now),
                path_exists: bookmark.path.exists(),
            })
        })
        .collect::<Vec<_>>();

    results.sort_by(|left, right| {
        right
            .fuzzy_score
            .cmp(&left.fuzzy_score)
            .then_with(|| right.access_score.cmp(&left.access_score))
            .then_with(|| {
                left.bookmark
                    .name
                    .to_lowercase()
                    .cmp(&right.bookmark.name.to_lowercase())
            })
            .then_with(|| left.bookmark.path.cmp(&right.bookmark.path))
    });
    results
}

fn access_score(bookmark: &Bookmark, now: DateTime<Utc>) -> i64 {
    let visits = bookmark.visit_count.min(10_000) as i64 * 100;
    let recency_bonus = match bookmark.last_visited_at {
        Some(last_visited_at) if last_visited_at >= now - Duration::days(7) => 1_000,
        Some(last_visited_at) if last_visited_at >= now - Duration::days(30) => 500,
        Some(last_visited_at) if last_visited_at >= now - Duration::days(180) => 100,
        Some(_) | None => 0,
    };
    visits + recency_bonus
}

#[cfg(test)]
mod tests {
    use super::{Query, query_bookmarks};
    use crate::model::{Bookmark, Database};
    use chrono::{DateTime, Duration, Utc};
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn query_matches_name_and_path_case_insensitively() {
        let db = fixture_database();
        let result = query_bookmarks(
            &db,
            Query {
                category: None,
                search: "proj",
            },
            Utc::now(),
        );
        assert_eq!(
            result
                .iter()
                .map(|result| result.bookmark.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Project"]
        );
    }

    #[test]
    fn query_matches_path_case_insensitively() {
        let db = fixture_database();
        let result = query_bookmarks(
            &db,
            Query {
                category: None,
                search: "WORK",
            },
            Utc::now(),
        );
        assert_eq!(
            result
                .iter()
                .map(|result| result.bookmark.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Project"]
        );
    }

    #[test]
    fn query_filters_by_category_before_searching() {
        let mut db = fixture_database();
        db.bookmarks[0].category = "personal".into();
        let result = query_bookmarks(
            &db,
            Query {
                category: Some("work"),
                search: "proj",
            },
            Utc::now(),
        );
        assert!(result.is_empty());
    }

    #[test]
    fn recent_frequently_visited_bookmark_ranks_first() {
        let now = Utc::now();
        let db = ranked_fixture(now);
        let result = query_bookmarks(&db, Query::default(), now);
        assert_eq!(result[0].bookmark.name, "active");
    }

    #[test]
    fn empty_search_returns_all_bookmarks_with_the_same_fuzzy_score() {
        let now = Utc::now();
        let db = ranked_fixture(now);
        let result = query_bookmarks(&db, Query::default(), now);
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|entry| entry.fuzzy_score == result[0].fuzzy_score)
        );
    }

    #[test]
    fn equal_scores_are_ordered_by_name_then_path() {
        let now = Utc::now();
        let db = equal_score_fixture();
        let result = query_bookmarks(&db, Query::default(), now);
        assert_eq!(names(&result), vec!["alpha", "beta", "beta"]);
        assert_eq!(result[1].bookmark.path, PathBuf::from("/tmp/a-beta"));
        assert_eq!(result[2].bookmark.path, PathBuf::from("/tmp/z-beta"));
    }

    #[test]
    fn access_score_caps_visits_and_treats_future_visits_as_recent() {
        let now = Utc::now();
        let db = Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![bookmark(
                "future",
                "/tmp/future",
                "default",
                20_000,
                Some(now + Duration::days(1)),
            )],
        };
        let result = query_bookmarks(&db, Query::default(), now);
        assert_eq!(result[0].access_score, 1_001_000);
    }

    #[test]
    fn access_score_uses_all_recency_windows() {
        let now = Utc::now();
        let db = Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![
                bookmark(
                    "week",
                    "/tmp/week",
                    "default",
                    0,
                    Some(now - Duration::days(7)),
                ),
                bookmark(
                    "month",
                    "/tmp/month",
                    "default",
                    0,
                    Some(now - Duration::days(30)),
                ),
                bookmark(
                    "half-year",
                    "/tmp/half-year",
                    "default",
                    0,
                    Some(now - Duration::days(180)),
                ),
                bookmark(
                    "old",
                    "/tmp/old",
                    "default",
                    0,
                    Some(now - Duration::days(181)),
                ),
                bookmark("never", "/tmp/never", "default", 0, None),
            ],
        };
        let result = query_bookmarks(&db, Query::default(), now);
        let score_for = |name| {
            result
                .iter()
                .find(|entry| entry.bookmark.name == name)
                .unwrap()
                .access_score
        };
        assert_eq!(score_for("week"), 1_000);
        assert_eq!(score_for("month"), 500);
        assert_eq!(score_for("half-year"), 100);
        assert_eq!(score_for("old"), 0);
        assert_eq!(score_for("never"), 0);
    }

    #[test]
    fn path_exists_reflects_the_current_file_system() {
        let temp = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let db = Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![
                bookmark("present", temp.path().to_str().unwrap(), "default", 0, None),
                bookmark("missing", "/tmp/mkd-query-missing", "default", 0, None),
            ],
        };
        let result = query_bookmarks(&db, Query::default(), now);
        assert!(
            result
                .iter()
                .find(|result| result.bookmark.name == "present")
                .unwrap()
                .path_exists
        );
        assert!(
            !result
                .iter()
                .find(|result| result.bookmark.name == "missing")
                .unwrap()
                .path_exists
        );
    }

    fn fixture_database() -> Database {
        Database {
            version: 1,
            categories: vec!["default".into(), "work".into()],
            bookmarks: vec![
                bookmark("Project", "/tmp/workspace", "work", 0, None),
                bookmark("notes", "/tmp/notes", "default", 0, None),
                bookmark("archive", "/tmp/archive", "default", 0, None),
            ],
        }
    }

    fn ranked_fixture(now: DateTime<Utc>) -> Database {
        Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![
                bookmark(
                    "inactive",
                    "/tmp/inactive",
                    "default",
                    2,
                    Some(now - Duration::days(200)),
                ),
                bookmark(
                    "active",
                    "/tmp/active",
                    "default",
                    10,
                    Some(now - Duration::days(1)),
                ),
            ],
        }
    }

    fn equal_score_fixture() -> Database {
        Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![
                bookmark("beta", "/tmp/z-beta", "default", 0, None),
                bookmark("alpha", "/tmp/alpha", "default", 0, None),
                bookmark("beta", "/tmp/a-beta", "default", 0, None),
            ],
        }
    }

    fn bookmark(
        name: &str,
        path: &str,
        category: &str,
        visit_count: u64,
        last_visited_at: Option<DateTime<Utc>>,
    ) -> Bookmark {
        Bookmark {
            id: Uuid::new_v4(),
            name: name.into(),
            path: PathBuf::from(path),
            category: category.into(),
            created_at: Utc::now(),
            last_visited_at,
            visit_count,
        }
    }

    fn names<'a>(results: &'a [super::QueryResult<'a>]) -> Vec<&'a str> {
        results
            .iter()
            .map(|result| result.bookmark.name.as_str())
            .collect()
    }
}
