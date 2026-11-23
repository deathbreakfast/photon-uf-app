//! Loading placeholders shared by the Photon pages.
//!
//! Each one keeps the shape of what it stands in for (real table headers, the
//! same cards and grids) and fills the values with Orbital `SkeletonItem`s.

use leptos::prelude::*;
use orbital::components::{
    AutoGrid, Card, CardContent, ContentContainer, Skeleton, SkeletonItem, SkeletonItemSize,
    SpacingSize, Subtitle2,
};
use orbital::primitives::{
    Flex, FlexGap, MaterialElevation, MaterialVariant, Table, TableBody, TableCell, TableHeader,
    TableHeaderCell, TableRow,
};

/// Rows a table skeleton draws when the real row count isn't known yet.
pub const SKELETON_ROWS: usize = 5;

/// A column of a [`TableSkeleton`]: header label and the width of its placeholder bar.
pub type SkeletonColumn = (&'static str, &'static str);

fn bar(width: &'static str, size: SkeletonItemSize) -> impl IntoView {
    view! { <SkeletonItem width=width size=Signal::from(size) /> }
}

/// Stack of placeholder lines, one per `(width, size)`.
fn lines(lines: &[(&'static str, SkeletonItemSize)]) -> impl IntoView {
    let bars = lines.iter().map(|&(w, s)| bar(w, s)).collect_view();
    view! {
        <Skeleton>
            <Flex vertical=true gap=FlexGap::Small>{bars}</Flex>
        </Skeleton>
    }
}

/// Table with its real header row and `rows` rows of placeholder cells.
#[component]
#[allow(clippy::needless_pass_by_value)]
pub fn TableSkeleton(
    /// Header label and placeholder width per column, in table order.
    columns: Vec<SkeletonColumn>,
    /// Number of placeholder rows.
    #[prop(default = SKELETON_ROWS)]
    rows: usize,
) -> impl IntoView {
    let (style_sheet, class_names) = turf::inline_style_sheet_values! {
        .Table { width: 100%; }
    };
    let headers = columns
        .iter()
        .map(|&(label, _)| view! { <TableHeaderCell>{label}</TableHeaderCell> })
        .collect_view();
    let body = (0..rows)
        .map(|_| {
            let cells = columns
                .iter()
                .map(|&(_, w)| view! { <TableCell>{bar(w, SkeletonItemSize::S16)}</TableCell> })
                .collect_view();
            view! { <TableRow>{cells}</TableRow> }
        })
        .collect_view();

    view! {
        <style>{style_sheet}</style>
        <Skeleton attr:data-testid="photon-table-skeleton">
            <Table class=class_names.table>
                <TableHeader>
                    <TableRow>{headers}</TableRow>
                </TableHeader>
                <TableBody>{body}</TableBody>
            </Table>
        </Skeleton>
    }
}

/// `Card` + `CardContent` with one placeholder line per line the loaded card prints.
#[component]
#[allow(clippy::needless_pass_by_value)]
pub fn LinesCardSkeleton(
    /// Width and height of each line.
    lines: Vec<(&'static str, SkeletonItemSize)>,
) -> impl IntoView {
    view! {
        <Card attr:data-testid="photon-card-skeleton">
            <CardContent>{self::lines(&lines)}</CardContent>
        </Card>
    }
}

/// Loading state for `PhotonStatsGrid`: the same grid with three label-and-value cards.
#[component]
pub fn StatsGridSkeleton() -> impl IntoView {
    let card = || {
        view! {
            <Card>
                <CardContent>
                    {lines(&[("72px", SkeletonItemSize::S12), ("48px", SkeletonItemSize::S28)])}
                </CardContent>
            </Card>
        }
    };
    view! {
        <AutoGrid attr:data-testid="photon-stats-skeleton" min="250px" gap=SpacingSize::Size160>
            {card()}
            {card()}
            {card()}
        </AutoGrid>
    }
}

/// Which card a [`ListSkeleton`] imitates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSkeletonKind {
    /// `TopicCard`: name, schema line, counts line.
    Topics,
    /// `SubscriptionCard`: name and badge, then four caption lines.
    Subscriptions,
}

/// Loading state for the topics and subscriptions lists: three card-shaped
/// placeholders inside the list's `Card`.
#[component]
pub fn ListSkeleton(
    /// The card shape to imitate.
    kind: ListSkeletonKind,
) -> impl IntoView {
    let shape: &[(&'static str, SkeletonItemSize)] = match kind {
        ListSkeletonKind::Topics => &[
            ("160px", SkeletonItemSize::S20),
            ("70%", SkeletonItemSize::S12),
            ("180px", SkeletonItemSize::S12),
        ],
        ListSkeletonKind::Subscriptions => &[
            ("180px", SkeletonItemSize::S20),
            ("140px", SkeletonItemSize::S12),
            ("110px", SkeletonItemSize::S12),
            ("120px", SkeletonItemSize::S12),
            ("130px", SkeletonItemSize::S12),
        ],
    };
    let cards = (0..3)
        .map(|_| {
            view! {
                <Card variant=MaterialVariant::Outlined elevation=MaterialElevation::Flat gap=FlexGap::Size(0)>
                    <CardContent>{lines(shape)}</CardContent>
                </Card>
            }
        })
        .collect_view();

    view! {
        <Card attr:data-testid="photon-list-skeleton">
            <CardContent>
                <Flex vertical=true gap=FlexGap::Medium>{cards}</Flex>
            </CardContent>
        </Card>
    }
}

/// Loading state for the event detail page: the meta lines, then Payload and
/// Actor headings over code-line placeholders.
#[component]
pub fn EventDetailSkeleton() -> impl IntoView {
    let code = |widths: &[&'static str]| {
        let rows: Vec<_> = widths.iter().map(|&w| (w, SkeletonItemSize::S12)).collect();
        view! {
            <Card>
                <CardContent>{lines(&rows)}</CardContent>
            </Card>
        }
    };
    view! {
        <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
            <LinesCardSkeleton lines=vec![
                ("150px", SkeletonItemSize::S16),
                ("120px", SkeletonItemSize::S16),
                ("60px", SkeletonItemSize::S16),
                ("220px", SkeletonItemSize::S16),
                ("90px", SkeletonItemSize::S16),
            ] />
            <Flex vertical=true gap=FlexGap::Small>
                <Subtitle2>"Payload"</Subtitle2>
                {code(&["16px", "240px", "80px", "180px", "16px"])}
            </Flex>
            <Flex vertical=true gap=FlexGap::Small>
                <Subtitle2>"Actor"</Subtitle2>
                {code(&["16px", "120px", "160px", "16px"])}
            </Flex>
        </Flex>
    }
}

/// Whole-page placeholder the layout shows while a route's code chunk downloads:
/// a title line and a card of lines, inside the `ContentContainer` every page uses.
#[component]
pub fn PageSkeleton() -> impl IntoView {
    view! {
        <ContentContainer data_testid="photon-page-skeleton">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Skeleton>{bar("200px", SkeletonItemSize::S28)}</Skeleton>
                <Card>
                    <CardContent>
                        <Skeleton>
                            <Flex vertical=true gap=FlexGap::Small>
                                {(0..6).map(|_| view! { <SkeletonItem /> }).collect_view()}
                            </Flex>
                        </Skeleton>
                    </CardContent>
                </Card>
            </Flex>
        </ContentContainer>
    }
}
