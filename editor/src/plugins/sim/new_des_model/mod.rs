mod car;
mod driving;
mod intersection;
mod parking;
mod queue;

pub use self::car::{Car, CarState};
pub use self::driving::DrivingSimState;
pub use self::intersection::IntersectionController;
pub use self::queue::Queue;
use geom::{Distance, Speed};
use map_model::{BuildingID, LaneID};
use serde_derive::{Deserialize, Serialize};
use sim::{CarID, VehicleType};

pub const MIN_VEHICLE_LENGTH: Distance = Distance::const_meters(2.0);
pub const MAX_VEHICLE_LENGTH: Distance = Distance::const_meters(7.0);
pub const FOLLOWING_DISTANCE: Distance = Distance::const_meters(1.0);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Vehicle {
    pub id: CarID,
    pub vehicle_type: VehicleType,

    pub length: Distance,
    pub max_speed: Option<Speed>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkingSpot {
    pub lane: LaneID,
    pub idx: usize,
}

impl ParkingSpot {
    pub fn new(lane: LaneID, idx: usize) -> ParkingSpot {
        ParkingSpot { lane, idx }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ParkedCar {
    pub car: CarID,
    pub spot: ParkingSpot,
    pub vehicle: Vehicle,
    pub owner: Option<BuildingID>,
}

impl ParkedCar {
    pub fn new(
        car: CarID,
        spot: ParkingSpot,
        vehicle: Vehicle,
        owner: Option<BuildingID>,
    ) -> ParkedCar {
        assert_eq!(vehicle.vehicle_type, VehicleType::Car);
        ParkedCar {
            car,
            spot,
            vehicle,
            owner,
        }
    }
}