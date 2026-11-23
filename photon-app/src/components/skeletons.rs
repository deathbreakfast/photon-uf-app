//! Loading placeholders shared by the Photon pages.
//!
//! Each one keeps the shape of what it stands in for (real table headers, the
//! same cards and grids) and fills the values with Orbital `SkeletonItem`s.
//! Every placeholder is one `Skeleton` (a single busy progressbar) carrying the
//! placeholder's `data-testid`.

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

/// A placeholder line: width (CSS) and height.
type Line = (&'static str, SkeletonItemSize);

fn bar(width: &'static str, size: SkeletonItemSize) -> impl IntoView {
    view! { <SkeletonItem width=width size=Signal::from(size) /> }
}

/// Stack of placeholder lines. Place inside a `Skeleton`.
fn lines(lines: Vec<Line>) -> impl IntoView {
    view! {
        <Flex vertical=true gap=FlexGap::Small>
            {lines.into_iter().map(|(w, s)| bar(w, s)).collect_view()}
        </Flex>
    }
}

/// Table with its real header row and `rows` rows of placeholder cells.
#[component]
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
    let columns = StoredValue::new(columns);

    view! {
        <style>{style_sheet}</style>
        <Skeleton attr:data-testid="photon-table-skeleton">
            <Table class=class_names.table>
                <TableHeader>
                    <TableRow>
                        {columns
                            .get_value()
                            .into_iter()
                            .map(|(label, _)| view! { <TableHeaderCell>{label}</TableHeaderCell> })
                            .collect_view()}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {(0..rows)
                        .map(|_| view! {
                            <TableRow>
                                {columns
                                    .get_value()
                                    .into_iter()
                                    .map(|(_, w)| view! { <TableCell>{bar(w, SkeletonItemSize::S16)}</TableCell> })
                                    .collect_view()}
                            </TableRow>
                        })
                        .collect_view()}
                </TableBody>
            </Table>
        </Skeleton>
    }
}

/// `Card` + `CardContent` with one placeholder line per line the loaded card prints.
#[component]
pub fn LinesCardSkeleton(
    /// Width and height of each line.
    lines: Vec<Line>,
) -> impl IntoView {
    view! {
        <Skeleton attr:data-testid="photon-card-skeleton">
            <Card>
                <CardContent>{self::lines(lines)}</CardContent>
            </Card>
        </Skeleton>
    }
}

/// Loading state for `PhotonStatsGrid`: the same grid with three label-and-value cards.
#[component]
pub fn StatsGridSkeleton() -> impl IntoView {
    view! {
        <Skeleton attr:data-testid="photon-stats-skeleton">
            <AutoGrid min="250px" gap=SpacingSize::Size160>
                {(0..3)
                    .map(|_| view! {
                        <Card>
                            <CardContent>
                                {lines(vec![("72px", SkeletonItemSize::S12), ("48px", SkeletonItemSize::S28)])}
                            </CardContent>
                        </Card>
                    })
                    .collect_view()}
            </AutoGrid>
        </Skeleton>
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

impl ListSkeletonKind {
    fn lines(self) -> Vec<Line> {
        match self {
            Self::Topics => vec![
                ("160px", SkeletonItemSize::S20),
                ("70%", SkeletonItemSize::S12),
                ("180px", SkeletonItemSize::S12),
            ],
            Self::Subscriptions => vec![
                ("180px", SkeletonItemSize::S20),
                ("140px", SkeletonItemSize::S12),
                ("110px", SkeletonItemSize::S12),
                ("120px", SkeletonItemSize::S12),
                ("130px", SkeletonItemSize::S12),
            ],
        }
    }
}

/// Loading state for the topics and subscriptions lists: three card-shaped
/// placeholders inside the list's `Card`.
#[component]
pub fn ListSkeleton(
    /// The card shape to imitate.
    kind: ListSkeletonKind,
) -> impl IntoView {
    view! {
        <Skeleton attr:data-testid="photon-list-skeleton">
            <Card>
                <CardContent>
                    <Flex vertical=true gap=FlexGap::Medium>
                        {(0..3)
                            .map(|_| view! {
                                <Card variant=MaterialVariant::Outlined elevation=MaterialElevation::Flat gap=FlexGap::Size(0)>
                                    <CardContent>{lines(kind.lines())}</CardContent>
                                </Card>
                            })
                            .collect_view()}
                    </Flex>
                </CardContent>
            </Card>
        </Skeleton>
    }
}

/// Code block placeholder for the event detail Payload and Actor sections.
fn code_lines(widths: &[&'static str]) -> impl IntoView {
    let rows: Vec<Line> = widths.iter().map(|&w| (w, SkeletonItemSize::S12)).collect();
    view! {
        <Card>
            <CardContent>{lines(rows)}</CardContent>
        </Card>
    }
}

/// Loading state for the event detail page: the meta lines, then Payload and
/// Actor headings over code-line placeholders.
#[component]
pub fn EventDetailSkeleton() -> impl IntoView {
    view! {
        <Skeleton attr:data-testid="photon-event-detail-skeleton">
            <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                <Card>
                    <CardContent>
                        {lines(vec![
                            ("150px", SkeletonItemSize::S16),
                            ("120px", SkeletonItemSize::S16),
                            ("60px", SkeletonItemSize::S16),
                            ("220px", SkeletonItemSize::S16),
                            ("90px", SkeletonItemSize::S16),
                        ])}
                    </CardContent>
                </Card>
                <Flex vertical=true gap=FlexGap::Small>
                    <Subtitle2>"Payload"</Subtitle2>
                    {code_lines(&["16px", "240px", "80px", "180px", "16px"])}
                </Flex>
                <Flex vertical=true gap=FlexGap::Small>
                    <Subtitle2>"Actor"</Subtitle2>
                    {code_lines(&["16px", "120px", "160px", "16px"])}
                </Flex>
            </Flex>
        </Skeleton>
    }
}

/// Whole-page placeholder the layout shows while a route's code chunk downloads:
/// a title line and a card of lines, inside the `ContentContainer` every page uses.
#[component]
pub fn PageSkeleton() -> impl IntoView {
    view! {
        <ContentContainer data_testid="photon-page-skeleton">
            <Skeleton>
                <Flex vertical=true gap=SpacingSize::Size240.flex_gap()>
                    {bar("200px", SkeletonItemSize::S28)}
                    <Card>
                        <CardContent>
                            <Flex vertical=true gap=FlexGap::Small>
                                {(0..6).map(|_| view! { <SkeletonItem /> }).collect_view()}
                            </Flex>
                        </CardContent>
                    </Card>
                </Flex>
            </Skeleton>
        </ContentContainer>
    }
}
