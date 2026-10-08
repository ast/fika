use anyhow::Result;

/// Minimal rig control: PTT and dial frequency.
pub trait Rig: Send {
    fn name(&self) -> String;
    fn ptt(&mut self, on: bool) -> Result<()>;
    fn frequency(&mut self) -> Result<Option<u64>>;
    fn set_data_mode(&mut self) -> Result<()> {
        Ok(())
    }
    fn connected(&self) -> bool;
}

/// No rig: PTT is a no-op (VOX, or no radio).
pub struct NoRig;

impl Rig for NoRig {
    fn name(&self) -> String {
        "no rig".into()
    }
    fn ptt(&mut self, _on: bool) -> Result<()> {
        Ok(())
    }
    fn frequency(&mut self) -> Result<Option<u64>> {
        Ok(None)
    }
    fn connected(&self) -> bool {
        true
    }
}
