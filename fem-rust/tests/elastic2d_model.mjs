// Model 2 case derived from implement_elastic2d.md; also used to generate examples.
const formulation=process.argv[2]??'planeStress';
const size=Number(process.argv[3]??'0.01');
const start=Number(process.argv[4]??'2000');
const end=Number(process.argv[5]??'20000');
const count=Number(process.argv[6]??'73');
const fieldFiles=process.argv[7]!=='false';
if(!['planeStress','planeStrain'].includes(formulation)||!(size>0&&start>0&&end>=start&&Number.isInteger(count)&&count>=2))throw Error('Invalid elastic 2D case.');
const q=(expression,unit)=>({expression:String(expression),unit});
const model={application:'fem',metadata:{id:`elastic_sheet_${formulation}`,name:`Free in-plane body: ${formulation}`},modelFormat:'fem-model/2',
 scenario:{dimension:2,physics:'elasticity',profileId:'elastic-2d'},'x-fullmeta-schema':'../../fem.schema.json',
 model:{units:'m',parameters:[{id:'frequency',name:'Frequency',symbol:'fscan',expression:String(start),unit:'Hz'}],
  variables:[],pointwiseVariables:[],integrationOperators:[],
  geometry:{tolerance:q('1e-9','m'),features:[{id:'body',name:'Rectangle',type:'rectangle',position:[q(0,'m'),q(0,'m')],size:[q(0.2,'m'),q(0.1,'m')]}]},
  selections:[{id:'domain',name:'Body',entity:'domain',addresses:[{membership:{body:'inside'}}]},
   {id:'drive',name:'Edge at x = 0',entity:'boundary',addresses:[{membership:{body:'boundary'}}],coordinateExpression:'x'}],
  materials:[{id:'material',name:'Illustrative elastic material',selectionId:'domain',density:q(2700,'kg/m^3'),youngModulus:q(7e10,'Pa'),poissonRatio:'0.33',densityLossFactor:'0.01'}],
  mesh:{elementFamily:'triangle',gmsh:{algorithm3D:1,elementOrder:1,optimize:true,optimizeNetgen:true,optimizeThreshold:0.25,smoothing:10,threads:1},minSize:q(size/2,'m'),maxSize:q(size,'m'),meshSizeControls:[],periodicPairs:[]},
  physics:{id:'physics',name:'In-plane elasticity',type:'elasticity',formulation,domainSelectionIds:['domain'],frequency:q('fscan','Hz'),
   load:{id:'load',name:'Transverse edge traction',type:'neumann',selectionId:'drive',value:q(0,'Pa'),fieldValues:[q(0,'Pa'),q(2500,'Pa')]}},
  study:[{id:'sweep',name:'Driven response',type:'frequencyDomain',physicsId:'physics',discretization:{elementOrder:1,fieldFormulation:formulation},solver:{backend:'faer',ordering:'default',threads:1,constitutiveQuadrature:'standard'},
   parameterSweep:{id:'axis',name:'Frequency',type:'parameterSweep',parameters:[{parameter:'fscan',values:{type:'linear',start:q(start,'Hz'),end:q(end,'Hz'),count}}]},output:{csvFilename:'response.csv',fieldFiles}}],
  results:[{id:'response',name:'Maximum transverse displacement',type:'sweep',quantity:'max_uy_magnitude_m'},
   {id:'field',name:'In-plane transverse displacement near first peak',type:'field',frequency:q(formulation==='planeStress'?8250:8750,'Hz'),quantity:{type:'fieldComponent',component:'uy',part:'magnitude'}}]}};
process.stdout.write(JSON.stringify(model,null,2)+'\n');
