// Application-owned polygonal CAD projection. Model quantities stay outside this module.
const dot=(a,b)=>a.reduce((sum,value,index)=>sum+value*b[index],0);
const subtract=(a,b)=>a.map((value,index)=>value-b[index]);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const length=a=>Math.hypot(...a);

export function polygon(vertices,surface) {
  const normal=cross(subtract(vertices[1],vertices[0]),subtract(vertices[2],vertices[0])),norm=length(normal);
  if(!(norm>0))return null;
  const unit=normal.map(value=>value/norm);
  return {vertices:vertices.map(vertex=>[...vertex]),surface,normal:unit,offset:dot(unit,vertices[0])};
}
const flipped=face=>({...face,vertices:[...face.vertices].reverse(),normal:face.normal.map(value=>-value),offset:-face.offset});

function split(face,plane,epsilon,coplanarFront,coplanarBack,front,back) {
  const types=face.vertices.map(vertex=>{const distance=dot(plane.normal,vertex)-plane.offset;return distance>epsilon?1:distance<-epsilon?2:0;});
  const type=types.reduce((value,type)=>value|type,0);
  if(type===0){(dot(face.normal,plane.normal)>=0?coplanarFront:coplanarBack).push(face);return;}
  if(type===1){front.push(face);return;}if(type===2){back.push(face);return;}
  const frontVertices=[],backVertices=[];
  face.vertices.forEach((a,index)=>{
    const next=(index+1)%face.vertices.length,b=face.vertices[next],left=types[index],right=types[next];
    if(left!==2)frontVertices.push(a);if(left!==1)backVertices.push(a);
    if((left|right)===3){const direction=subtract(b,a),fraction=(plane.offset-dot(plane.normal,a))/dot(plane.normal,direction),point=a.map((value,axis)=>value+direction[axis]*fraction);frontVertices.push(point);backVertices.push(point);}
  });
  for(const [vertices,destination] of [[frontVertices,front],[backVertices,back]]){
    const distinct=vertices.filter((vertex,index)=>length(subtract(vertex,vertices[(index+vertices.length-1)%vertices.length]))>epsilon);
    if(distinct.length>=3)destination.push({...face,vertices:distinct});
  }
}

class Partition {
  constructor(faces=[],epsilon){this.epsilon=epsilon;this.faces=[];this.front=null;this.back=null;this.plane=null;this.build(faces);}
  build(faces){
    if(!faces.length)return;this.plane||={normal:[...faces[0].normal],offset:faces[0].offset};const front=[],back=[];
    faces.forEach(face=>split(face,this.plane,this.epsilon,this.faces,this.faces,front,back));
    if(front.length){this.front||=new Partition([],this.epsilon);this.front.build(front);}
    if(back.length){this.back||=new Partition([],this.epsilon);this.back.build(back);}
  }
  all(){return [...this.faces,...(this.front?.all()||[]),...(this.back?.all()||[])];}
  invert(){this.faces=this.faces.map(flipped);if(this.plane)this.plane={normal:this.plane.normal.map(value=>-value),offset:-this.plane.offset};this.front?.invert();this.back?.invert();[this.front,this.back]=[this.back,this.front];}
  clip(faces){if(!this.plane)return faces;let front=[],back=[];faces.forEach(face=>split(face,this.plane,this.epsilon,front,back,front,back));if(this.front)front=this.front.clip(front);back=this.back?this.back.clip(back):[];return [...front,...back];}
  clipTo(other){this.faces=other.clip(this.faces);this.front?.clipTo(other);this.back?.clipTo(other);}
}

export function combineSolids(left,right,operation,epsilon) {
  if(!['union','intersection'].includes(operation))throw new Error('Unsupported solid operation.');
  if(!left.length||!right.length)return operation==='union'?[...left,...right]:[];
  const a=new Partition(left,epsilon),b=new Partition(right,epsilon);
  if(operation==='union'){
    a.clipTo(b);b.clipTo(a);b.invert();b.clipTo(a);b.invert();a.build(b.all());return a.all();
  }
  a.invert();b.clipTo(a);b.invert();a.clipTo(b);b.clipTo(a);a.build(b.all());a.invert();return a.all();
}

export function boxSolid(position,size,id){
  const corners=Array.from({length:8},(_,index)=>position.map((value,axis)=>value+size[axis]*((index>>axis)&1)));
  return [[0,4,6,2],[1,3,7,5],[0,1,5,4],[2,6,7,3],[0,2,3,1],[4,5,7,6]].map((face,index)=>polygon(face.map(node=>corners[node]),`${id}:face:${index}`));
}

export function circularBasis(direction){
  if(!Array.isArray(direction)||direction.length!==3||direction.some(value=>typeof value!=='number'||!Number.isFinite(value)))throw new Error('Circular geometry requires three finite direction components.');
  const norm=length(direction);if(!(norm>0)||!Number.isFinite(norm))throw new Error('Circular geometry has an invalid direction.');
  const normal=direction.map(value=>value/norm),seed=Math.abs(normal[0])<.8?[1,0,0]:[0,1,0],raw=cross(normal,seed),u=raw.map(value=>value/length(raw));
  return {normal,u,v:cross(normal,u)};
}

export function circlePoints(center,normal,radius,count=48){
  const {u,v}=circularBasis(normal);
  return Array.from({length:count},(_,index)=>center.map((value,axis)=>value+radius*(u[axis]*Math.cos(index*2*Math.PI/count)+v[axis]*Math.sin(index*2*Math.PI/count))));
}

export function cylinderSolid(base,axis,radius,height,id){
  const {normal}=circularBasis(axis),lower=circlePoints(base,normal,radius),upper=lower.map(point=>point.map((value,index)=>value+normal[index]*height));
  const faces=[polygon([...lower].reverse(),`${id}:cap:0`),polygon(upper,`${id}:cap:1`)];
  for(let index=0;index<lower.length;index++){const next=(index+1)%lower.length;faces.push(polygon([lower[index],lower[next],upper[next],upper[index]],`${id}:wall`));}
  return faces;
}

/** Split T junctions before removing planar partition edges and cylinder seams. */
export function solidOutlines(faces,epsilon){
  const vertices=faces.flatMap(face=>face.vertices),fragments=new Map();
  const vertexKey=point=>point.map(value=>Math.round(value/epsilon)).join(',');
  for(const face of faces)for(let index=0;index<face.vertices.length;index++){
    const a=face.vertices[index],b=face.vertices[(index+1)%face.vertices.length],direction=subtract(b,a),squared=dot(direction,direction);
    if(squared<=epsilon*epsilon)continue;
    const fractions=[0,1];
    for(const vertex of vertices){const relative=subtract(vertex,a),fraction=dot(relative,direction)/squared;
      if(fraction>0&&fraction<1&&length(cross(relative,direction))<=epsilon*Math.sqrt(squared))fractions.push(fraction);
    }
    fractions.sort((a,b)=>a-b);
    for(let part=1;part<fractions.length;part++){
      if((fractions[part]-fractions[part-1])*Math.sqrt(squared)<=epsilon)continue;
      const endpoints=[fractions[part-1],fractions[part]].map(fraction=>a.map((value,axis)=>value+direction[axis]*fraction));
      const key=endpoints.map(vertexKey).sort().join('|'),existing=fragments.get(key);
      if(existing)existing.faces.push(face);else fragments.set(key,{points:endpoints,faces:[face]});
    }
  }
  return [...fragments.values()].filter(edge=>edge.faces.length===1||edge.faces.some((face,index)=>index>0&&face.surface!==edge.faces[0].surface&&Math.abs(dot(face.normal,edge.faces[0].normal))<1-1e-10)).map(edge=>edge.points);
}
