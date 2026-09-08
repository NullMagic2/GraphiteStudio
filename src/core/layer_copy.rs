use super::{document::{Document,DrawingLayer},orientation::QuarterTurn};

#[derive(Debug,Clone)]
pub(crate) struct LayerCopy {
    source:u64,
    copy:DrawingLayer,
}
impl LayerCopy {
    pub fn prepare(doc:&mut Document)->Option<Self> {
        if doc.layers.len()>=256{return None;}
        let mut copy=doc.layers[doc.active_layer_index()].clone();
        let source=copy.id;
        copy.deposit.capture_from_surface(&doc.surface);
        let base:String=copy.name.chars().take(80).collect();
        copy.name=format!("{base} copy");let mut suffix=2;
        while doc.layers.iter().any(|l|l.name==copy.name) {
            copy.name=format!("{base} copy {suffix}");suffix+=1;
        }
        while doc.layers.iter().any(|l|l.id==doc.next_layer_id) || doc.next_layer_id==0 {
            doc.next_layer_id=doc.next_layer_id.wrapping_add(1).max(2);
        }
        copy.id=doc.next_layer_id;
        doc.next_layer_id=doc.next_layer_id.wrapping_add(1).max(2);
        Some(Self{source,copy})
    }
    pub fn apply(&self,doc:&mut Document,redo:bool)->bool {
        let Some(source)=doc.layers.iter().position(|l|l.id==self.source) else{return false;};
        let existing=doc.layers.iter().position(|l|l.id==self.copy.id);
        if redo {
            if existing.is_some() || doc.layers.len()>=256{return false;}
            doc.activate_layer(source);
            doc.layers.insert(source+1,self.copy.clone());
            doc.activate_layer(source+1);
        }else{
            let Some(index)=existing else{return false;};
            doc.activate_layer(index);
            doc.remove_active_layer();
            doc.activate_layer_by_id(self.source);
        }
        doc.select_layer(doc.active_layer_index(),false,false);
        doc.selection=Default::default();
        doc.mark_all_dirty();
        true
    }
    pub fn rotate(&mut self,turn:&mut QuarterTurn) {
        self.copy.deposit=turn.sparse(&self.copy.deposit);
        self.copy.vectors=turn.layer(&self.copy.vectors);
    }
}
