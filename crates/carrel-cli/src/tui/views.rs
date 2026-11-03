//! The views of the Library and of a collection (`docs/design/library-home.md`): `v` on the
//! Library or in Browse cycles through them, scoped to the whole archive or to the chosen
//! collection and those inside it, as the apps' view switcher does. The Library's own view is
//! its covers, and a collection's its items in Browse; the others show where Discover's pages
//! did. `w` on the Library, or on a collection in Browse, wanders from a random item there.
//! Esc goes back. The last view of the Library and of each collection is remembered while the
//! interactive mode runs.

use carrel_core::library::View;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use std::collections::HashMap;

use super::app::{App, Tab};
use super::discover::View as Page;
use super::item::Pane;
use super::review::Handled;

/// Where adding and the views were opened from, and the views last shown.
#[derive(Debug)]
pub struct Modes {
    /// The tab `+` was pressed on, which Esc goes back to from adding.
    pub before_adding: Option<Tab>,
    /// Where the views were opened from, the Library or Browse, which Esc goes back to.
    pub views_from: Tab,
    /// The last view of the Library (`None`) and of each collection, for next time.
    pub remembered: HashMap<Option<String>, View>,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            before_adding: None,
            views_from: Tab::Library,
            remembered: HashMap::new(),
        }
    }
}

/// The page that draws `view`, for every view but the scope's own.
fn page(view: View) -> Option<Page> {
    match view {
        View::Covers => None,
        View::Items => Some(Page::Items),
        View::Timeline => Some(Page::Timeline),
        View::Map => Some(Page::Map),
        View::Connections => Some(Page::Graph),
        View::Activity => Some(Page::Activity),
    }
}

/// The view a page shows, for remembering it; on this day and a wander aren't views.
fn view_of(page: Page) -> Option<View> {
    match page {
        Page::Items => Some(View::Items),
        Page::Timeline => Some(View::Timeline),
        Page::Map => Some(View::Map),
        Page::Graph => Some(View::Connections),
        Page::Activity => Some(View::Activity),
        Page::Today | Page::Wander => None,
    }
}

impl App {
    /// The keys of the modes on any screen: adding (`+`) and the views (`v`).
    pub(super) fn mode_key(&mut self, key: KeyEvent) -> Handled {
        if self.adding_key(key) {
            return Handled::Yes(None);
        }
        self.views_key(key).unwrap_or(Handled::No)
    }

    /// The collection `v` and `w` act on in Browse: the chosen one.
    fn browsed_collection(&self) -> Option<String> {
        self.collections
            .get(self.browse_collection)
            .map(|c| c.path.clone())
    }

    /// The scope's own view: the Library's covers, or a collection's items.
    fn own_view(scope: Option<&str>) -> View {
        View::of(scope)[0]
    }

    /// Shows `view` of `scope`, from `from`: the scope's own view is its tab, the others the
    /// views' page.
    pub(super) fn show_view(&mut self, view: View, scope: Option<String>, from: Tab) {
        self.modes.remembered.insert(scope.clone(), view);
        self.modes.views_from = from;
        let Some(page) = page(view).filter(|_| view != Self::own_view(scope.as_deref())) else {
            self.tab = from;
            return;
        };
        self.stacks.scope = scope;
        self.stacks.view = page;
        self.stacks.scroll = 0;
        self.tab = Tab::Discover;
        self.load_discover();
    }

    /// The views' keys on the Library, in Browse and on the views' page: `v` the next view,
    /// `w` a wander from a random item, Esc back. `None` when the key isn't one of them here.
    pub(super) fn views_key(&mut self, key: KeyEvent) -> Option<Handled> {
        let (scope, from) = match self.tab {
            Tab::Library => (None, Tab::Library),
            Tab::Browse if self.browse_pane != Pane::Item && self.search.is_none() => {
                (self.browsed_collection(), Tab::Browse)
            }
            Tab::Discover => (self.stacks.scope.clone(), self.modes.views_from),
            _ => return None,
        };
        match key.code {
            KeyCode::Char('v') => {
                let views = View::of(scope.as_deref());
                let now = match self.tab {
                    Tab::Discover => view_of(self.stacks.view).unwrap_or(views[0]),
                    _ => views[0],
                };
                // From the scope's own view, its last other view if it has one.
                let remembered = self
                    .modes
                    .remembered
                    .get(&scope)
                    .copied()
                    .filter(|v| self.tab != Tab::Discover && *v != views[0]);
                let next = remembered.unwrap_or_else(|| now.next_in(views));
                self.show_view(next, scope, from);
            }
            // In Browse's items, `w` wanders from the chosen item instead.
            KeyCode::Char('w')
                if self.tab == Tab::Library
                    || (self.tab == Tab::Browse && self.browse_pane == Pane::Collections) =>
            {
                self.stacks.scope = scope;
                self.modes.views_from = from;
                self.wander_anywhere();
            }
            KeyCode::Esc if self.tab == Tab::Discover => {
                self.modes
                    .remembered
                    .insert(scope.clone(), Self::own_view(scope.as_deref()));
                self.tab = from;
            }
            _ => return None,
        }
        Some(Handled::Yes(None))
    }

    /// Going to the Library shows its last view.
    pub(super) fn library_shown(&mut self) {
        if let Some(view) = self.modes.remembered.get(&None).copied()
            && view != View::Covers
        {
            self.show_view(view, None, Tab::Library);
        }
    }

    /// What the views' page is looking at, for its title: "Library" or the collection.
    pub fn views_title(&self) -> String {
        let scope = self.stacks.scope.as_deref().unwrap_or("Library");
        let view = view_of(self.stacks.view).map_or(
            match self.stacks.view {
                Page::Wander => "Wander",
                _ => "On This Day",
            },
            View::title,
        );
        format!("{scope} · {view}")
    }
}

#[cfg(test)]
mod tests {
    use carrel_core::library::View;
    use ratatui::crossterm::event::KeyCode;

    use super::super::app::Tab;
    use super::super::discover::View as Page;
    use super::super::item::Pane;
    use super::super::tests::{key, sample, screen};

    #[test]
    fn v_cycles_the_librarys_views_and_esc_goes_back() {
        let (_dir, mut app) = sample(false);
        app.key(key(KeyCode::Char('v')));
        assert_eq!((app.tab, app.stacks.view), (Tab::Discover, Page::Items));
        assert_eq!(app.stacks.scope, None);
        app.key(key(KeyCode::Char('v')));
        assert_eq!(app.stacks.view, Page::Timeline);
        let text = screen(&app, 120, 30);
        assert!(text.contains("Library · Timeline"), "{text}");
        assert!(text.contains("1 Library"), "the views aren't a tab");
        assert!(!text.contains("Discover"), "{text}");
        for _ in 0..4 {
            app.key(key(KeyCode::Char('v')));
        }
        assert_eq!(app.tab, Tab::Library, "after Activity, back to the covers");

        // The last view is remembered.
        app.key(key(KeyCode::Char('v')));
        app.key(key(KeyCode::Char('v')));
        app.key(key(KeyCode::Char('5')));
        app.key(key(KeyCode::Char('1')));
        assert_eq!((app.tab, app.stacks.view), (Tab::Discover, Page::Timeline));
        app.key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Library);
        assert!(!app.quit);
    }

    #[test]
    fn a_collections_views_look_only_inside_it() {
        let (_dir, mut app) = sample(false);
        app.key(key(KeyCode::Char('5')));
        assert_eq!(app.browse_pane, Pane::Collections);
        let path = app.collections[app.browse_collection].path.clone();
        app.key(key(KeyCode::Char('v')));
        assert_eq!((app.tab, app.stacks.view), (Tab::Discover, Page::Timeline));
        assert_eq!(app.stacks.scope.as_deref(), Some(path.as_str()));
        app.show_view(View::Map, Some("Reading".into()), Tab::Browse);
        let map = app.stacks.lines.join("\n");
        assert!(
            map.contains("Reading") && !map.contains("Projects"),
            "{map}"
        );
        app.key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Browse);
        // Items is the collection's own view: Browse.
        app.show_view(View::Items, Some("Reading".into()), Tab::Browse);
        assert_eq!(app.tab, Tab::Browse);
    }

    #[test]
    fn w_wanders_from_a_random_item_in_the_scope() {
        let (_dir, mut app) = sample(false);
        app.key(key(KeyCode::Char('w')));
        assert_eq!((app.tab, app.stacks.view), (Tab::Discover, Page::Wander));
        assert_ne!(app.stacks.step, Vec::<String>::new());
        app.key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Library);
    }
}
