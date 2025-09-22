import { Group } from "../interface/gov";

export class MemDb {
    groups: Map<number, Group>;
    
    constructor() {
        this.groups = new Map<number, Group>
    }


}
