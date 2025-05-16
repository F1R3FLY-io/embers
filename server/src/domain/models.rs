#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum RequestStatus {
    Done,
    Ongoing,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub date: u64,
    pub amount: u64,
    pub status: RequestStatus,
}

#[derive(Debug, Clone)]
pub struct Exchange {}

#[derive(Debug, Clone)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone)]
pub struct Boost {
    pub id: String,
    pub username: String,
    pub direction: Direction,
    pub date: u64,
    pub amount: u64,
    pub post: String,
}

#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub date: u64,
    pub amount: u64,
    pub to_address: String,
    pub cost: u64,
}

#[derive(Debug, Clone)]
pub struct WalletStateAndHistory {
    pub address: String,
    pub balance: u64,
    pub requests: Vec<Request>,
    pub exchanges: Vec<Exchange>,
    pub boosts: Vec<Boost>,
    pub transfers: Vec<Transfer>,
}

#[derive(Debug, Clone)]
pub struct Cost(pub u64);
