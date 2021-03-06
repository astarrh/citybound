use super::{PI, N, P2, V2, WithUniqueOrthogonal, angle_along_to, signed_angle_to, RoughEq,
Intersect, Intersection, IntersectionResult, HasBoundingBox, BoundingBox, THICKNESS};
use nalgebra::Rotation2;

pub trait Curve: Sized {
    fn project_with_max_distance(
        &self,
        point: P2,
        max_distance: N,
        tolerance: N,
    ) -> Option<(N, P2)> {
        self.project_with_tolerance(point, tolerance)
            .and_then(|(offset, projected_point)| {
                if (point - projected_point).norm() < max_distance {
                    Some((offset, projected_point))
                } else {
                    None
                }
            })
    }
    fn project_with_tolerance(&self, point: P2, tolerance: N) -> Option<(N, P2)>;
    fn project(&self, point: P2) -> Option<(N, P2)> {
        self.project_with_tolerance(point, THICKNESS)
    }
    fn includes(&self, point: P2) -> bool {
        self.distance_to(point) < THICKNESS / 2.0
    }
    fn distance_to(&self, point: P2) -> N;
}

pub trait FiniteCurve: Curve {
    fn length(&self) -> N;
    fn along(&self, distance: N) -> P2;
    fn direction_along(&self, distance: N) -> V2;
    fn start(&self) -> P2;
    fn start_direction(&self) -> V2 {
        self.direction_along(0.0)
    }
    fn end(&self) -> P2;
    fn end_direction(&self) -> V2 {
        self.direction_along(self.length())
    }
    fn midpoint(&self) -> P2 {
        self.along(self.length() / 2.0)
    }
    fn midpoint_direction(&self) -> V2 {
        self.direction_along(self.length() / 2.0)
    }
    fn reverse(&self) -> Self;
    fn subsection(&self, start: N, end: N) -> Option<Self>;
    fn shift_orthogonally(&self, shift_to_right: N) -> Option<Self>;
}

#[derive(Copy, Clone, Debug)]
pub struct Circle {
    pub center: P2,
    pub radius: N,
}

impl Curve for Circle {
    fn project_with_tolerance(&self, point: P2, _tolerance: N) -> Option<(N, P2)> {
        let angle = angle_along_to(V2::new(1.0, 0.0), V2::new(0.0, 1.0), point - self.center);
        Some((
            self.radius * angle,
            self.center + (point - self.center).normalize() * self.radius,
        ))
    }

    fn distance_to(&self, point: P2) -> N {
        ((point - self.center).norm() - self.radius).abs()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Line {
    pub start: P2,
    pub direction: V2,
}

impl Curve for Line {
    fn project_with_tolerance(&self, point: P2, _tolerance: N) -> Option<(N, P2)> {
        let along = (point - self.start).dot(&self.direction);
        Some((along, self.start + along * self.direction))
    }

    fn distance_to(&self, point: P2) -> N {
        (point - self.start).dot(&self.direction.orthogonal()).abs()
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct Segment {
    pub start: P2,
    pub center_or_direction: V2,
    pub end: P2,
    pub length: N,
    signed_radius: N,
}

const DIRECTION_TOLERANCE: N = 0.0001;
pub const MIN_START_TO_END: N = 0.01;
const MAX_SIMPLE_LINE_LENGTH: N = 0.5;

fn start_end_invalid(start: P2, end: P2) -> bool {
    start.x.is_nan()
        || start.y.is_nan()
        || end.x.is_nan()
        || end.y.is_nan()
        || start.rough_eq_by(end, MIN_START_TO_END)
}

impl Segment {
    pub fn line(start: P2, end: P2) -> Option<Segment> {
        if start_end_invalid(start, end) {
            None
        } else {
            Some(Segment {
                start,
                center_or_direction: (end - start).normalize(),
                end,
                length: (end - start).norm(),
                signed_radius: 0.0,
            })
        }
    }

    pub fn arc_with_direction(start: P2, direction: V2, end: P2) -> Option<Segment> {
        if start_end_invalid(start, end) {
            None
        } else if direction.rough_eq_by((end - start).normalize(), DIRECTION_TOLERANCE) {
            Segment::line(start, end)
        } else {
            let signed_radius = {
                let half_chord = (end - start) / 2.0;
                half_chord.norm_squared() / direction.orthogonal().dot(&half_chord)
            };
            let center = start + signed_radius * direction.orthogonal();
            let angle_span = angle_along_to(start - center, direction, end - center);
            Some(Segment {
                start,
                center_or_direction: center.coords,
                end,
                length: angle_span * signed_radius.abs(),
                signed_radius,
            })
        }
    }

    pub fn biarc(
        start: P2,
        start_direction: V2,
        end: P2,
        end_direction: V2,
    ) -> Option<Vec<Segment>> {
        if start_end_invalid(start, end) {
            return None;
        }
        let simple_curve = Segment::arc_with_direction(start, start_direction, end)?;
        if simple_curve
            .end_direction()
            .rough_eq_by(end_direction, DIRECTION_TOLERANCE)
        {
            Some(vec![simple_curve])
        } else if (end - start).norm() < MAX_SIMPLE_LINE_LENGTH {
            Some(vec![Segment::line(start, end)?])
        } else {
            let maybe_linear_intersection = match (
                &Line {
                    start,
                    direction: start_direction,
                },
                &Line {
                    start: end,
                    direction: -end_direction,
                },
            ).intersect()
            {
                IntersectionResult::Intersecting(intersections) => {
                    if intersections[0].along_a > 0.0 && intersections[0].along_b > 0.0 {
                        Some(intersections[0])
                    } else {
                        None
                    }
                }
                IntersectionResult::Apart => None,
                IntersectionResult::Coincident => unreachable!(),
            };

            let (connection_position, connection_direction) = if let Some(Intersection {
                position,
                ..
            }) = maybe_linear_intersection
            {
                let start_to_intersection_distance = (start - position).norm();
                let end_to_intersection_distance = (end - position).norm();

                if start_to_intersection_distance < end_to_intersection_distance {
                    // arc then line
                    (
                        position + start_to_intersection_distance * end_direction,
                        end_direction,
                    )
                } else {
                    // line then arc
                    (
                        position + end_to_intersection_distance * -start_direction,
                        start_direction,
                    )
                }
            } else {
                // http://www.ryanjuckett.com/programming/biarc-interpolation/
                let v = end - start;
                let t = start_direction + end_direction;
                let same_direction =
                    start_direction.rough_eq_by(end_direction, DIRECTION_TOLERANCE);
                let end_orthogonal_of_start = v.dot(&end_direction).rough_eq(0.0);

                if same_direction && end_orthogonal_of_start {
                    //    __
                    //   /  \
                    //  ^    v    ^
                    //        \__/
                    (
                        P2::from_coordinates((start.coords + end.coords) / 2.0),
                        -start_direction,
                    )
                } else {
                    let d = if same_direction {
                        v.dot(&v) / (4.0 * v.dot(&end_direction))
                    } else {
                        // magic - I'm pretty sure this can be simplified
                        let v_dot_t = v.dot(&t);
                        (-v_dot_t
                            + (v_dot_t * v_dot_t
                                + 2.0 * (1.0 - start_direction.dot(&end_direction)) * v.dot(&v))
                                .sqrt())
                            / (2.0 * (1.0 - start_direction.dot(&end_direction)))
                    };

                    let start_offset_point = start + d * start_direction;
                    let end_offset_point = end - d * end_direction;
                    let connection_direction = (end_offset_point - start_offset_point).normalize();

                    (
                        start_offset_point + d * connection_direction,
                        connection_direction,
                    )
                }
            };

            if start.rough_eq_by(connection_position, MIN_START_TO_END) {
                Some(vec![Segment::arc_with_direction(
                    connection_position,
                    connection_direction,
                    end,
                )?])
            } else if end.rough_eq_by(connection_position, MIN_START_TO_END) {
                Some(vec![Segment::arc_with_direction(
                    start,
                    start_direction,
                    connection_position,
                )?])
            } else {
                Some(vec![
                    Segment::arc_with_direction(start, start_direction, connection_position)?,
                    Segment::arc_with_direction(connection_position, connection_direction, end)?,
                ])
            }
        }
    }

    pub fn is_linear(&self) -> bool {
        self.signed_radius == 0.0
    }

    pub fn center(&self) -> P2 {
        P2::from_coordinates(self.center_or_direction)
    }

    pub fn radius(&self) -> N {
        self.signed_radius.abs()
    }

    pub fn signed_angle(&self) -> N {
        self.length / self.signed_radius
    }

    pub fn winding_angle(&self, point: P2) -> N {
        let simple_angle = signed_angle_to(self.start - point, self.end - point);

        if self.is_linear() {
            simple_angle
        } else {
            // simple
            //     P
            //  A      B
            // ;        ;
            //  \      /
            //   '-..-'

            //     P
            //  A      B
            //   '-..-'

            //  A      B
            // ;        ;
            //  \      /
            //   '-..-'
            //        P

            //
            //  A      B
            //   '-..-'
            //        P

            // inverse & long
            //  A      B
            // ;   P    ;
            //  \      /
            //   '-..-'

            //
            //  A      B
            //   '-P.-'

            let chord_midpoint = P2::from_coordinates((self.start.coords + self.end.coords) / 2.0);
            let sagitta_direction =
                (self.end() - self.start()).normalize().orthogonal() * self.signed_radius.signum();
            let on_correct_side_of_chord = (self.center() - chord_midpoint).dot(&sagitta_direction)
                < (self.center() - point).dot(&sagitta_direction);

            let between_chord_and_arc =
                (point - self.center()).norm() < self.radius() && on_correct_side_of_chord;

            if between_chord_and_arc {
                (2.0 * PI - simple_angle.abs()) * -simple_angle.signum()
            } else {
                simple_angle
            }
        }
    }

    pub fn to_svg(&self) -> String {
        if self.is_linear() {
            format!(
                "M {} {} L {} {}",
                self.start.x, self.start.y, self.end.x, self.end.y
            )
        } else {
            format!(
                "M {} {} A {} {} 0 {} {} {} {}",
                self.start.x,
                self.start.y,
                self.radius(),
                self.radius(),
                if self.length / self.radius() > PI {
                    1
                } else {
                    0
                },
                if self.signed_radius < 0.0 { 1 } else { 0 },
                self.end.x,
                self.end.y
            )
        }
    }
}

impl FiniteCurve for Segment {
    fn length(&self) -> N {
        self.length
    }

    fn along(&self, distance: N) -> P2 {
        if self.is_linear() {
            self.start + distance * self.center_or_direction
        } else {
            let center_to_start = self.start - self.center();
            let angle_to_rotate = distance / -self.signed_radius;
            let center_to_point = Rotation2::new(angle_to_rotate) * center_to_start;
            self.center() + center_to_point
        }
    }

    fn direction_along(&self, distance: N) -> V2 {
        if self.is_linear() {
            self.center_or_direction
        } else {
            let center_to_start = self.start - self.center();
            let angle_to_rotate = distance / -self.signed_radius;
            let center_to_point = Rotation2::new(angle_to_rotate) * center_to_start;
            center_to_point.normalize().orthogonal() * self.signed_radius.signum()
        }
    }

    fn start(&self) -> P2 {
        self.start
    }

    fn start_direction(&self) -> V2 {
        if self.is_linear() {
            self.center_or_direction
        } else {
            let center_to_start = self.start - self.center();
            center_to_start.normalize().orthogonal() * self.signed_radius.signum()
        }
    }

    fn end(&self) -> P2 {
        self.end
    }

    fn end_direction(&self) -> V2 {
        if self.is_linear() {
            self.center_or_direction
        } else {
            let center_to_end = self.end - self.center();
            center_to_end.normalize().orthogonal() * self.signed_radius.signum()
        }
    }

    fn reverse(&self) -> Segment {
        if self.is_linear() {
            Segment::line(self.end, self.start)
        } else {
            Segment::arc_with_direction(self.end, -self.end_direction(), self.start)
        }.expect("Reversing a segment should always produce a valid segment")
    }

    fn subsection(&self, start: N, end: N) -> Option<Segment> {
        let true_start = start.max(0.0);
        let true_end = end.min(self.length);

        if true_end - true_start < MIN_START_TO_END {
            None
        } else if self.is_linear() || true_end.rough_eq(0.0) || true_start.rough_eq(self.length) {
            Segment::line(self.along(true_start), self.along(true_end))
        } else {
            Segment::arc_with_direction(
                self.along(true_start),
                self.direction_along(true_start),
                self.along(true_end),
            )
        }
    }

    fn shift_orthogonally(&self, shift_to_right: N) -> Option<Segment> {
        if self.is_linear() {
            let offset = self.start_direction().orthogonal() * shift_to_right;
            Segment::line(self.start + offset, self.end + offset)
        } else {
            let start = self.start + self.start_direction().orthogonal() * shift_to_right;
            let end = self.end + self.end_direction().orthogonal() * shift_to_right;
            Segment::arc_with_direction(start, self.start_direction(), end)
        }
    }
}

impl Curve for Segment {
    fn project_with_tolerance(&self, point: P2, tolerance: N) -> Option<(N, P2)> {
        if (point - self.start).norm() < tolerance {
            Some((0.0, self.start))
        } else if (point - self.end).norm() < tolerance {
            Some((self.length, self.end))
        } else if self.is_linear() {
            let direction = self.center_or_direction;
            let line_offset = direction.dot(&(point - self.start));
            if line_offset >= 0.0 && line_offset <= self.length {
                Some((
                    line_offset,
                    self.start + line_offset * self.center_or_direction,
                ))
            } else {
                None
            }
        } else {
            let angle_start_to_point = angle_along_to(
                self.start - self.center(),
                self.start_direction(),
                point - self.center(),
            );

            let angle_span = self.length / self.radius();

            if angle_start_to_point <= angle_span {
                let point = self.center() + (point - self.center()).normalize() * self.radius();
                Some((
                    (angle_start_to_point * self.radius()).min(self.length),
                    point,
                ))
            } else {
                None
            }
        }
    }

    fn distance_to(&self, point: P2) -> N {
        match self.project(point) {
            Some((_offset, projected_point)) => (point - projected_point).norm(),
            None => (self.start - point).norm().min((self.end - point).norm()),
        }
    }
}

impl<'a> RoughEq for &'a Segment {
    fn rough_eq_by(&self, other: &Segment, tolerance: N) -> bool {
        self.start.rough_eq_by(other.start, tolerance)
            && self.end.rough_eq_by(other.end, tolerance)
            && self.midpoint().rough_eq_by(other.midpoint(), tolerance)
    }
}

impl HasBoundingBox for Segment {
    fn bounding_box(&self) -> BoundingBox {
        if self.is_linear() {
            BoundingBox {
                min: P2::new(
                    self.start.x.min(self.end.x) - THICKNESS,
                    self.start.y.min(self.end.y) - THICKNESS,
                ),
                max: P2::new(
                    self.start.x.max(self.end.x) + THICKNESS,
                    self.start.y.max(self.end.y) + THICKNESS,
                ),
            }
        } else {
            let half_diagonal = V2::new(self.radius() + THICKNESS, self.radius() + THICKNESS);
            BoundingBox {
                min: self.center() - half_diagonal,
                max: self.center() + half_diagonal,
            }
        }
    }
}

impl ::std::fmt::Debug for Segment {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        if self.is_linear() {
            write!(
                f,
                "LineSeg({:.4}, {:.4} to {:.4}, {:.4})",
                self.start().x,
                self.start().y,
                self.end().x,
                self.end().y
            )
        } else {
            write!(
                f,
                "ArcSeg({:.4}, {:.4} around {:.4}, {:.4} to {:.4}, {:.4})",
                self.start().x,
                self.start().y,
                self.center().x,
                self.center().y,
                self.end().x,
                self.end().y
            )
        }
    }
}

#[cfg(test)]
mod tests;
