// SPDX-License-Identifier: GPL-3.0-or-later

//! Unsupported console commands return explicit reasons instead of claiming success.

use super::{catalog, Console};
use crate::Game;

macro_rules! rejects {
    ($test:ident, $name:literal) => {
        #[test]
        fn $test() {
            let mut game = Game::new(soe_esm::LoadOrder::new());
            let mut console = Console::new();
            let expected = catalog::entry($name).unwrap();
            let error = console.run(&mut game, $name).unwrap_err();
            assert_eq!(error, format!("{}: {}", expected.name, expected.reason));
        }
    };
}

rejects!(abs_returns_reason, "abs");
rejects!(actor_returns_reason, "actor");
rejects!(addachievement_returns_reason, "addachievement");
rejects!(adddecal_returns_reason, "adddecal");
rejects!(addfaceanimnote_returns_reason, "addfaceanimnote");
rejects!(addflames_returns_reason, "addflames");
rejects!(addformtoformlist_returns_reason, "addformtoformlist");
rejects!(addformtoleveledlist_returns_reason, "addformtoleveledlist");
rejects!(addmusic_returns_reason, "addmusic");
rejects!(addnote_returns_reason, "addnote");
rejects!(addocclusionplane_returns_reason, "addocclusionplane");
rejects!(addtocontainer_returns_reason, "addtocontainer");
rejects!(addtomap_returns_reason, "addtomap");
rejects!(addtopic_returns_reason, "addtopic");
rejects!(addwatchaddress_returns_reason, "addwatchaddress");
#[test]
fn animcam_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "animcam").unwrap(),
        super::Outcome::Done
    );
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("animcam".into())]
    );
}
rejects!(applyhavokimpulse_returns_reason, "applyhavokimpulse");
rejects!(
    applyimagespacemodifier_returns_reason,
    "applyimagespacemodifier"
);
rejects!(
    applyimagespacemodifiercrossfade_returns_reason,
    "applyimagespacemodifiercrossfade"
);
rejects!(attachashpile_returns_reason, "attachashpile");
rejects!(attachpapyrusscript_returns_reason, "attachpapyrusscript");
rejects!(
    autodisplayobjectives_returns_reason,
    "autodisplayobjectives"
);
rejects!(autosave_returns_reason, "autosave");
rejects!(begintrace_returns_reason, "begintrace");
rejects!(betacomment_returns_reason, "betacomment");
rejects!(calclowpathtopoint_returns_reason, "calclowpathtopoint");
#[test]
fn calcpathtopoint_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "calcpathtopoint").unwrap_err(),
        "CalcPathToPoint: expected x y z in Skyrim units"
    );
}
rejects!(camerashake_returns_reason, "camerashake");
rejects!(canhaveflames_returns_reason, "canhaveflames");
rejects!(canpaycrimegold_returns_reason, "canpaycrimegold");
rejects!(castimmediateonself_returns_reason, "castimmediateonself");
rejects!(cellinfo_returns_reason, "cellinfo");
#[test]
fn checkmemory_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    let super::Outcome::List(rows) = console.run(&mut game, "checkmemory").unwrap() else {
        panic!("allocation report required");
    };
    assert_eq!(rows.len(), soe_trace::SYSTEM_COUNT);
    for (row, system) in rows.iter().zip(soe_trace::System::ALL) {
        assert!(row.starts_with(&format!("{} Rust live_bytes=", system.name())));
        assert!(row.contains(" allocation_calls=") && row.contains(" allocated_bytes="));
    }
}
rejects!(checkplayerdoors_returns_reason, "checkplayerdoors");
rejects!(clearachievement_returns_reason, "clearachievement");
rejects!(
    clearactorsfactionsplayerenemyflag_returns_reason,
    "clearactorsfactionsplayerenemyflag"
);
rejects!(clearadaptedlight_returns_reason, "clearadaptedlight");
rejects!(cleararreststate_returns_reason, "cleararreststate");
rejects!(clearconsole_returns_reason, "clearconsole");
rejects!(cleardestruction_returns_reason, "cleardestruction");
rejects!(
    clearfactionplayerenemyflag_returns_reason,
    "clearfactionplayerenemyflag"
);
rejects!(clearlocationmarkers_returns_reason, "clearlocationmarkers");
rejects!(
    clearnoactivationsound_returns_reason,
    "clearnoactivationsound"
);
rejects!(clearquestaliases_returns_reason, "clearquestaliases");
rejects!(closefile_returns_reason, "closefile");
rejects!(collisionmesh_returns_reason, "collisionmesh");
rejects!(copysaves_returns_reason, "copysaves");
rejects!(cos_returns_reason, "cos");
rejects!(createdetectionevent_returns_reason, "createdetectionevent");
rejects!(createevent_returns_reason, "createevent");
rejects!(creategamedata_returns_reason, "creategamedata");
rejects!(creategrassat_returns_reason, "creategrassat");
rejects!(createsavedata_returns_reason, "createsavedata");
rejects!(csize_returns_reason, "csize");
rejects!(damageobject_returns_reason, "damageobject");
rejects!(debugcombatbehavior_returns_reason, "debugcombatbehavior");
rejects!(disableallactors_returns_reason, "disableallactors");
rejects!(disableallmines_returns_reason, "disableallmines");
rejects!(
    disablelinkedpathpoints_returns_reason,
    "disablelinkedpathpoints"
);
rejects!(displaygraphvariable_returns_reason, "displaygraphvariable");
rejects!(displaylookikdebug_returns_reason, "displaylookikdebug");
rejects!(doesnotexist_returns_reason, "doesnotexist");
rejects!(dotrap_returns_reason, "dotrap");
rejects!(drawskeleton_returns_reason, "drawskeleton");
rejects!(dropme_returns_reason, "dropme");
rejects!(dualcast_returns_reason, "dualcast");
rejects!(
    dumpmasterfileseekdata_returns_reason,
    "dumpmasterfileseekdata"
);
rejects!(dumpmodelmap_returns_reason, "dumpmodelmap");
rejects!(dumpniupdates_returns_reason, "dumpniupdates");
rejects!(dumppapyrusstacks_returns_reason, "dumppapyrusstacks");
rejects!(dumppapyrusupdates_returns_reason, "dumppapyrusupdates");
rejects!(dumptexturepalette_returns_reason, "dumptexturepalette");
rejects!(duplicatenpcstats_returns_reason, "duplicatenpcstats");
rejects!(effectwasdualcast_returns_reason, "effectwasdualcast");
rejects!(emptyrefalias_returns_reason, "emptyrefalias");
rejects!(enablefast_returns_reason, "enablefast");
rejects!(enablefasttravel_returns_reason, "enablefasttravel");
rejects!(
    enablelinkedpathpoints_returns_reason,
    "enablelinkedpathpoints"
);
rejects!(enablerumble_returns_reason, "enablerumble");
rejects!(
    enablestorymanagerlogging_returns_reason,
    "enablestorymanagerlogging"
);
rejects!(entertrigger_returns_reason, "entertrigger");
rejects!(
    epalchemyeffecthaskeyword_returns_reason,
    "epalchemyeffecthaskeyword"
);
rejects!(
    epalchemygetmakingpoison_returns_reason,
    "epalchemygetmakingpoison"
);
rejects!(
    eptemperingitemhaskeyword_returns_reason,
    "eptemperingitemhaskeyword"
);
rejects!(
    eptemperingitemisenchanted_returns_reason,
    "eptemperingitemisenchanted"
);
rejects!(equipobject_returns_reason, "equipobject");
rejects!(essentialdeathreload_returns_reason, "essentialdeathreload");
rejects!(evalactortextures_returns_reason, "evalactortextures");
rejects!(
    evaluatespellconditions_returns_reason,
    "evaluatespellconditions"
);
rejects!(exists_returns_reason, "exists");
rejects!(
    exportinventoryiteminfo_returns_reason,
    "exportinventoryiteminfo"
);
rejects!(
    exportperftrackingdata_returns_reason,
    "exportperftrackingdata"
);
rejects!(fadesfx_returns_reason, "fadesfx");
rejects!(family_returns_reason, "family");
rejects!(fasttravel_returns_reason, "fasttravel");
rejects!(findform_returns_reason, "findform");
rejects!(fireweapon_returns_reason, "fireweapon");
rejects!(forcefilecache_returns_reason, "forcefilecache");
rejects!(forceflee_returns_reason, "forceflee");
rejects!(forcelocintoalias_returns_reason, "forcelocintoalias");
rejects!(forceoutofmemory_returns_reason, "forceoutofmemory");
rejects!(
    forceradiostationupdate_returns_reason,
    "forceradiostationupdate"
);
rejects!(forcerefintoalias_returns_reason, "forcerefintoalias");
rejects!(forcersxcrash_returns_reason, "forcersxcrash");
rejects!(forcesave_returns_reason, "forcesave");
rejects!(
    freezerenderaccumulation_returns_reason,
    "freezerenderaccumulation"
);
rejects!(gamecomment_returns_reason, "gamecomment");
rejects!(getactionref_returns_reason, "getactionref");
rejects!(getactivationheight_returns_reason, "getactivationheight");
rejects!(
    getactoraggroradiusviolated_returns_reason,
    "getactoraggroradiusviolated"
);
rejects!(
    getactorcrimeplayerenemy_returns_reason,
    "getactorcrimeplayerenemy"
);
rejects!(getactorsinhigh_returns_reason, "getactorsinhigh");
rejects!(getalarmed_returns_reason, "getalarmed");
rejects!(getaliasedref_returns_reason, "getaliasedref");
rejects!(
    getallowworldinteractions_returns_reason,
    "getallowworldinteractions"
);
rejects!(getamountsoldstolen_returns_reason, "getamountsoldstolen");
rejects!(getanimaction_returns_reason, "getanimaction");
rejects!(getarrestedstate_returns_reason, "getarrestedstate");
rejects!(getarrestingactor_returns_reason, "getarrestingactor");
rejects!(getattacked_returns_reason, "getattacked");
rejects!(getattackstate_returns_reason, "getattackstate");
rejects!(getbribeamount_returns_reason, "getbribeamount");
rejects!(getbribesuccess_returns_reason, "getbribesuccess");
rejects!(getbroadcaststate_returns_reason, "getbroadcaststate");
rejects!(getbuttonpressed_returns_reason, "getbuttonpressed");
rejects!(getcannibal_returns_reason, "getcannibal");
rejects!(getcauseofdeath_returns_reason, "getcauseofdeath");
rejects!(getclassdefaultmatch_returns_reason, "getclassdefaultmatch");
rejects!(getclothingvalue_returns_reason, "getclothingvalue");
rejects!(
    getcombatgroupmembercount_returns_reason,
    "getcombatgroupmembercount"
);
rejects!(getcombatstate_returns_reason, "getcombatstate");
rejects!(getcombattarget_returns_reason, "getcombattarget");
rejects!(
    getcombattargethaskeyword_returns_reason,
    "getcombattargethaskeyword"
);
rejects!(getconcussed_returns_reason, "getconcussed");
rejects!(getcontainer_returns_reason, "getcontainer");
rejects!(getcrime_returns_reason, "getcrime");
rejects!(
    getcrimegoldnonviolent_returns_reason,
    "getcrimegoldnonviolent"
);
rejects!(getcrimegoldviolent_returns_reason, "getcrimegoldviolent");
rejects!(getcrimeknown_returns_reason, "getcrimeknown");
rejects!(getcurrentaipackage_returns_reason, "getcurrentaipackage");
rejects!(
    getcurrentaiprocedure_returns_reason,
    "getcurrentaiprocedure"
);
rejects!(
    getcurrentcastingtype_returns_reason,
    "getcurrentcastingtype"
);
rejects!(
    getcurrentdeliverytype_returns_reason,
    "getcurrentdeliverytype"
);
rejects!(
    getcurrentshoutvariation_returns_reason,
    "getcurrentshoutvariation"
);
rejects!(getcurrenttime_returns_reason, "getcurrenttime");
rejects!(
    getcurrentweatherpercent_returns_reason,
    "getcurrentweatherpercent"
);
rejects!(getdayofweek_returns_reason, "getdayofweek");
rejects!(getdaysinjail_returns_reason, "getdaysinjail");
rejects!(getdeadcount_returns_reason, "getdeadcount");
rejects!(getdefaultopen_returns_reason, "getdefaultopen");
rejects!(getdestroyed_returns_reason, "getdestroyed");
rejects!(getdestructionstage_returns_reason, "getdestructionstage");
rejects!(getdetected_returns_reason, "getdetected");
rejects!(getdialogueemotion_returns_reason, "getdialogueemotion");
rejects!(
    getdialogueemotionvalue_returns_reason,
    "getdialogueemotionvalue"
);
rejects!(getdisease_returns_reason, "getdisease");
rejects!(
    getdistancefromactorspath_returns_reason,
    "getdistancefromactorspath"
);
rejects!(getequipped_returns_reason, "getequipped");
rejects!(getequippeditemtype_returns_reason, "getequippeditemtype");
rejects!(getequippedshout_returns_reason, "getequippedshout");
rejects!(geteventdata_returns_reason, "geteventdata");
rejects!(
    getfactioncombatreaction_returns_reason,
    "getfactioncombatreaction"
);
rejects!(
    getfactionrankdifference_returns_reason,
    "getfactionrankdifference"
);
rejects!(getfactionreaction_returns_reason, "getfactionreaction");
rejects!(getfactionrelation_returns_reason, "getfactionrelation");
rejects!(getflyingstate_returns_reason, "getflyingstate");
rejects!(getforcerun_returns_reason, "getforcerun");
rejects!(getforcesneak_returns_reason, "getforcesneak");
rejects!(getfriendhit_returns_reason, "getfriendhit");
rejects!(getfurnituremarkerid_returns_reason, "getfurnituremarkerid");
rejects!(getgiftgivenvalue_returns_reason, "getgiftgivenvalue");
rejects!(getgold_returns_reason, "getgold");
rejects!(
    getgraphvariablefloat_returns_reason,
    "getgraphvariablefloat"
);
rejects!(getgraphvariableint_returns_reason, "getgraphvariableint");
rejects!(getgroupmembercount_returns_reason, "getgroupmembercount");
rejects!(getgrouptargetcount_returns_reason, "getgrouptargetcount");
rejects!(gethasnote_returns_reason, "gethasnote");
rejects!(getheadingangle_returns_reason, "getheadingangle");
rejects!(gethealthpercentage_returns_reason, "gethealthpercentage");
rejects!(
    gethighestrelationshiprank_returns_reason,
    "gethighestrelationshiprank"
);
rejects!(gethitlocation_returns_reason, "gethitlocation");
rejects!(getidledoneonce_returns_reason, "getidledoneonce");
rejects!(getignorecrime_returns_reason, "getignorecrime");
rejects!(
    getignorefriendlyhits_returns_reason,
    "getignorefriendlyhits"
);
rejects!(getincell_returns_reason, "getincell");
rejects!(getincellparam_returns_reason, "getincellparam");
rejects!(getincontainer_returns_reason, "getincontainer");
rejects!(getincurrentloc_returns_reason, "getincurrentloc");
rejects!(getincurrentlocalias_returns_reason, "getincurrentlocalias");
rejects!(
    getincurrentlocformlist_returns_reason,
    "getincurrentlocformlist"
);
rejects!(getinfaction_returns_reason, "getinfaction");
rejects!(getinfamy_returns_reason, "getinfamy");
rejects!(getinfamynonviolent_returns_reason, "getinfamynonviolent");
rejects!(getinfamyviolent_returns_reason, "getinfamyviolent");
rejects!(getinsamecell_returns_reason, "getinsamecell");
rejects!(
    getinsharedcrimefaction_returns_reason,
    "getinsharedcrimefaction"
);
rejects!(getintimidatesuccess_returns_reason, "getintimidatesuccess");
rejects!(getinworldspace_returns_reason, "getinworldspace");
rejects!(getinzone_returns_reason, "getinzone");
rejects!(getisalerted_returns_reason, "getisalerted");
rejects!(getisaliasref_returns_reason, "getisaliasref");
rejects!(getisalignment_returns_reason, "getisalignment");
rejects!(getisclass_returns_reason, "getisclass");
rejects!(getisclassdefault_returns_reason, "getisclassdefault");
rejects!(
    getiscrashlandrequest_returns_reason,
    "getiscrashlandrequest"
);
rejects!(getiscreature_returns_reason, "getiscreature");
rejects!(getiscreaturetype_returns_reason, "getiscreaturetype");
rejects!(getiscrimefaction_returns_reason, "getiscrimefaction");
rejects!(getiscurrentpackage_returns_reason, "getiscurrentpackage");
rejects!(getiscurrentweather_returns_reason, "getiscurrentweather");
rejects!(getiseditorlocalias_returns_reason, "getiseditorlocalias");
rejects!(getiseditorlocation_returns_reason, "getiseditorlocation");
rejects!(getisflying_returns_reason, "getisflying");
rejects!(getisformtype_returns_reason, "getisformtype");
rejects!(
    getishastylandrequest_returns_reason,
    "getishastylandrequest"
);
rejects!(getisid_returns_reason, "getisid");
rejects!(getisinjured_returns_reason, "getisinjured");
rejects!(getislockbroken_returns_reason, "getislockbroken");
rejects!(getisobjecttype_returns_reason, "getisobjecttype");
rejects!(getisplayablerace_returns_reason, "getisplayablerace");
rejects!(getisrace_returns_reason, "getisrace");
rejects!(getisreference_returns_reason, "getisreference");
rejects!(getissex_returns_reason, "getissex");
rejects!(getisuseditem_returns_reason, "getisuseditem");
rejects!(
    getisuseditemequiptype_returns_reason,
    "getisuseditemequiptype"
);
rejects!(getisuseditemtype_returns_reason, "getisuseditemtype");
rejects!(getisvoicetype_returns_reason, "getisvoicetype");
rejects!(getitemhealthpercent_returns_reason, "getitemhealthpercent");
rejects!(
    getkeyworddataforalias_returns_reason,
    "getkeyworddataforalias"
);
rejects!(
    getkeyworddataforcurrentlocation_returns_reason,
    "getkeyworddataforcurrentlocation"
);
rejects!(
    getkeyworddataforlocation_returns_reason,
    "getkeyworddataforlocation"
);
rejects!(getkeyworditemcount_returns_reason, "getkeyworditemcount");
rejects!(getkillingblowlimb_returns_reason, "getkillingblowlimb");
rejects!(getknockedstate_returns_reason, "getknockedstate");
rejects!(getknockstateenum_returns_reason, "getknockstateenum");
rejects!(getlastbumpdirection_returns_reason, "getlastbumpdirection");
rejects!(getlasthitcritical_returns_reason, "getlasthitcritical");
rejects!(getlastplayeraction_returns_reason, "getlastplayeraction");
rejects!(
    getleveledencountervalue_returns_reason,
    "getleveledencountervalue"
);
rejects!(getlightlevel_returns_reason, "getlightlevel");
rejects!(getlineofsight_returns_reason, "getlineofsight");
rejects!(getlinkedref_returns_reason, "getlinkedref");
rejects!(
    getlocaliasreftypealivecount_returns_reason,
    "getlocaliasreftypealivecount"
);
rejects!(
    getlocaliasreftypedeadcount_returns_reason,
    "getlocaliasreftypedeadcount"
);
rejects!(
    getlocationaliascleared_returns_reason,
    "getlocationaliascleared"
);
rejects!(getlos_returns_reason, "getlos");
rejects!(
    getlowestrelationshiprank_returns_reason,
    "getlowestrelationshiprank"
);
rejects!(getmapmarkervisible_returns_reason, "getmapmarkervisible");
rejects!(getmemcheckpoint_returns_reason, "getmemcheckpoint");
rejects!(getmovementdirection_returns_reason, "getmovementdirection");
rejects!(getmovementspeed_returns_reason, "getmovementspeed");
rejects!(
    getnobleedoutrecovery_returns_reason,
    "getnobleedoutrecovery"
);
rejects!(getnorumors_returns_reason, "getnorumors");
rejects!(
    getnumericpackagedata_returns_reason,
    "getnumericpackagedata"
);
rejects!(getoffersservicesnow_returns_reason, "getoffersservicesnow");
rejects!(getopenstate_returns_reason, "getopenstate");
rejects!(getpackagetarget_returns_reason, "getpackagetarget");
rejects!(getpairedanimation_returns_reason, "getpairedanimation");
rejects!(getparentref_returns_reason, "getparentref");
rejects!(
    getpathingcurrentspeed_returns_reason,
    "getpathingcurrentspeed"
);
rejects!(
    getpathingcurrentspeedangle_returns_reason,
    "getpathingcurrentspeedangle"
);
rejects!(
    getpathingtargetangleoffset_returns_reason,
    "getpathingtargetangleoffset"
);
rejects!(
    getpathingtargetoffset_returns_reason,
    "getpathingtargetoffset"
);
rejects!(
    getpathingtargetspeed_returns_reason,
    "getpathingtargetspeed"
);
rejects!(
    getpathingtargetspeedangle_returns_reason,
    "getpathingtargetspeedangle"
);
rejects!(getpcenemyoffaction_returns_reason, "getpcenemyoffaction");
rejects!(getpcexpelled_returns_reason, "getpcexpelled");
rejects!(getpcfactionattack_returns_reason, "getpcfactionattack");
rejects!(getpcfactionmurder_returns_reason, "getpcfactionmurder");
rejects!(getpcinfaction_returns_reason, "getpcinfaction");
rejects!(getpcisclass_returns_reason, "getpcisclass");
rejects!(getpcisrace_returns_reason, "getpcisrace");
rejects!(getpcissex_returns_reason, "getpcissex");
rejects!(getpcsleephours_returns_reason, "getpcsleephours");
rejects!(getpersuasionnumber_returns_reason, "getpersuasionnumber");
rejects!(getplantedexplosive_returns_reason, "getplantedexplosive");
rejects!(getplayeraction_returns_reason, "getplayeraction");
rejects!(
    getplayercontrolsdisabled_returns_reason,
    "getplayercontrolsdisabled"
);
rejects!(getplayergrabbedref_returns_reason, "getplayergrabbedref");
#[test]
fn getplayername_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "getplayername").unwrap(),
        super::Outcome::Text("Player".into())
    );
}
rejects!(getplayerteammate_returns_reason, "getplayerteammate");
rejects!(
    getplayerteammatecount_returns_reason,
    "getplayerteammatecount"
);
rejects!(getquestvariable_returns_reason, "getquestvariable");
rejects!(getrandompercent_returns_reason, "getrandompercent");
rejects!(getrealhourspassed_returns_reason, "getrealhourspassed");
rejects!(getreceivedgiftvalue_returns_reason, "getreceivedgiftvalue");
rejects!(getreftypealivecount_returns_reason, "getreftypealivecount");
rejects!(getreftypedeadcount_returns_reason, "getreftypedeadcount");
rejects!(getrelativeangle_returns_reason, "getrelativeangle");
rejects!(getreplaceditemtype_returns_reason, "getreplaceditemtype");
rejects!(getrestrained_returns_reason, "getrestrained");
rejects!(getscriptvariable_returns_reason, "getscriptvariable");
rejects!(getsecondspassed_returns_reason, "getsecondspassed");
rejects!(getshouldattack_returns_reason, "getshouldattack");
rejects!(getshouldhelp_returns_reason, "getshouldhelp");
rejects!(getsitting_returns_reason, "getsitting");
rejects!(getsleeping_returns_reason, "getsleeping");
rejects!(getspellusagenum_returns_reason, "getspellusagenum");
rejects!(getstamina_returns_reason, "getstamina");
rejects!(getstaminapercentage_returns_reason, "getstaminapercentage");
rejects!(getstartingangle_returns_reason, "getstartingangle");
rejects!(getstartingpos_returns_reason, "getstartingpos");
rejects!(getstolenitemvalue_returns_reason, "getstolenitemvalue");
rejects!(
    getstolenitemvaluenocrime_returns_reason,
    "getstolenitemvaluenocrime"
);
rejects!(gettalkedtopc_returns_reason, "gettalkedtopc");
rejects!(gettalkedtopcparam_returns_reason, "gettalkedtopcparam");
rejects!(gettargetheight_returns_reason, "gettargetheight");
rejects!(getthreatratio_returns_reason, "getthreatratio");
rejects!(gettimedead_returns_reason, "gettimedead");
rejects!(
    gettotalpersuasionnumber_returns_reason,
    "gettotalpersuasionnumber"
);
rejects!(
    gettrespasswarninglevel_returns_reason,
    "gettrespasswarninglevel"
);
rejects!(getunconscious_returns_reason, "getunconscious");
rejects!(getvampirefeed_returns_reason, "getvampirefeed");
rejects!(getvatsbackareafree_returns_reason, "getvatsbackareafree");
rejects!(
    getvatsbacktargetvisible_returns_reason,
    "getvatsbacktargetvisible"
);
rejects!(getvatsfrontareafree_returns_reason, "getvatsfrontareafree");
rejects!(
    getvatsfronttargetvisible_returns_reason,
    "getvatsfronttargetvisible"
);
rejects!(getvatsleftareafree_returns_reason, "getvatsleftareafree");
rejects!(
    getvatslefttargetvisible_returns_reason,
    "getvatslefttargetvisible"
);
rejects!(getvatsmode_returns_reason, "getvatsmode");
rejects!(getvatsrightareafree_returns_reason, "getvatsrightareafree");
rejects!(
    getvatsrighttargetvisible_returns_reason,
    "getvatsrighttargetvisible"
);
rejects!(getvatstargetheight_returns_reason, "getvatstargetheight");
rejects!(getvatsvalue_returns_reason, "getvatsvalue");
rejects!(getvelocity_returns_reason, "getvelocity");
rejects!(getvmquestvariable_returns_reason, "getvmquestvariable");
rejects!(getvmscriptvariable_returns_reason, "getvmscriptvariable");
rejects!(getwalk_returns_reason, "getwalk");
rejects!(getwalkspeed_returns_reason, "getwalkspeed");
rejects!(getwantblocking_returns_reason, "getwantblocking");
rejects!(getweaponanimtype_returns_reason, "getweaponanimtype");
rejects!(getwindspeed_returns_reason, "getwindspeed");
rejects!(getwithindistance_returns_reason, "getwithindistance");
rejects!(
    getwithinpackagelocation_returns_reason,
    "getwithinpackagelocation"
);
rejects!(getxpfornextlevel_returns_reason, "getxpfornextlevel");
rejects!(gotojail_returns_reason, "gotojail");
rejects!(hairtint_returns_reason, "hairtint");
rejects!(hasassociationtype_returns_reason, "hasassociationtype");
rejects!(
    hasassociationtypeany_returns_reason,
    "hasassociationtypeany"
);
rejects!(hasbeeneaten_returns_reason, "hasbeeneaten");
rejects!(
    hasboundweaponequipped_returns_reason,
    "hasboundweaponequipped"
);
rejects!(hasequippedspell_returns_reason, "hasequippedspell");
rejects!(
    hasfamilyrelationship_returns_reason,
    "hasfamilyrelationship"
);
rejects!(
    hasfamilyrelationshipany_returns_reason,
    "hasfamilyrelationshipany"
);
rejects!(hasflames_returns_reason, "hasflames");
rejects!(haskeyword_returns_reason, "haskeyword");
rejects!(haslinkedref_returns_reason, "haslinkedref");
rejects!(hasloaded3d_returns_reason, "hasloaded3d");
rejects!(hasmagiceffect_returns_reason, "hasmagiceffect");
rejects!(
    hasmagiceffectkeyword_returns_reason,
    "hasmagiceffectkeyword"
);
rejects!(
    hasparentrelationship_returns_reason,
    "hasparentrelationship"
);
rejects!(hasreftype_returns_reason, "hasreftype");
rejects!(
    hassameeditorlocasref_returns_reason,
    "hassameeditorlocasref"
);
rejects!(
    hassameeditorlocasrefalias_returns_reason,
    "hassameeditorlocasrefalias"
);
rejects!(
    hastwohandedweaponequipped_returns_reason,
    "hastwohandedweaponequipped"
);
rejects!(havokvdbcapture_returns_reason, "havokvdbcapture");
rejects!(hidemenu_returns_reason, "hidemenu");
rejects!(ignorecrime_returns_reason, "ignorecrime");
rejects!(ignorerenderpass_returns_reason, "ignorerenderpass");
rejects!(incmemcheckpoint_returns_reason, "incmemcheckpoint");
rejects!(installmemorytracker_returns_reason, "installmemorytracker");
rejects!(interruptcast_returns_reason, "interruptcast");
rejects!(isactionref_returns_reason, "isactionref");
rejects!(isactor_returns_reason, "isactor");
rejects!(isactoravictim_returns_reason, "isactoravictim");
rejects!(isactorsaioff_returns_reason, "isactorsaioff");
rejects!(isactorusingatorch_returns_reason, "isactorusingatorch");
rejects!(isallowedtofly_returns_reason, "isallowedtofly");
rejects!(isanimplaying_returns_reason, "isanimplaying");
rejects!(isattacking_returns_reason, "isattacking");
rejects!(isattacktype_returns_reason, "isattacktype");
rejects!(isbeingridden_returns_reason, "isbeingridden");
rejects!(isbleedingout_returns_reason, "isbleedingout");
rejects!(isbribedbyplayer_returns_reason, "isbribedbyplayer");
rejects!(iscarryable_returns_reason, "iscarryable");
rejects!(iscasting_returns_reason, "iscasting");
rejects!(iscellowner_returns_reason, "iscellowner");
rejects!(ischild_returns_reason, "ischild");
rejects!(isclosertoathanb_returns_reason, "isclosertoathanb");
rejects!(iscloudy_returns_reason, "iscloudy");
rejects!(iscombattarget_returns_reason, "iscombattarget");
rejects!(iscommandedactor_returns_reason, "iscommandedactor");
rejects!(
    iscontinuingpackagepcnear_returns_reason,
    "iscontinuingpackagepcnear"
);
rejects!(
    iscurrentfurnitureobj_returns_reason,
    "iscurrentfurnitureobj"
);
rejects!(
    iscurrentfurnitureref_returns_reason,
    "iscurrentfurnitureref"
);
rejects!(iscurrentspell_returns_reason, "iscurrentspell");
rejects!(isdualcasting_returns_reason, "isdualcasting");
rejects!(
    isenteringinteractionquick_returns_reason,
    "isenteringinteractionquick"
);
rejects!(isexitinginstant_returns_reason, "isexitinginstant");
rejects!(
    isexitinginteractionquick_returns_reason,
    "isexitinginteractionquick"
);
rejects!(isfacingup_returns_reason, "isfacingup");
rejects!(isfleeing_returns_reason, "isfleeing");
rejects!(isfurnitureanimtype_returns_reason, "isfurnitureanimtype");
rejects!(isfurnitureentrytype_returns_reason, "isfurnitureentrytype");
rejects!(isgoredisabled_returns_reason, "isgoredisabled");
rejects!(isgreetingplayer_returns_reason, "isgreetingplayer");
rejects!(isguard_returns_reason, "isguard");
rejects!(ishorsestolen_returns_reason, "ishorsestolen");
rejects!(ishostiletoactor_returns_reason, "ishostiletoactor");
rejects!(isidleplaying_returns_reason, "isidleplaying");
rejects!(isignoringcombat_returns_reason, "isignoringcombat");
rejects!(isimagespaceactive_returns_reason, "isimagespaceactive");
rejects!(isincriticalstage_returns_reason, "isincriticalstage");
rejects!(isindangerouswater_returns_reason, "isindangerouswater");
rejects!(
    isindialoguewithplayer_returns_reason,
    "isindialoguewithplayer"
);
rejects!(isinfavorstate_returns_reason, "isinfavorstate");
rejects!(
    isinfriendstatewithplayer_returns_reason,
    "isinfriendstatewithplayer"
);
rejects!(isinfurniturestate_returns_reason, "isinfurniturestate");
rejects!(isininterior_returns_reason, "isininterior");
rejects!(isinlist_returns_reason, "isinlist");
rejects!(isinmyownedcell_returns_reason, "isinmyownedcell");
rejects!(
    isinsamecurrentlocasref_returns_reason,
    "isinsamecurrentlocasref"
);
rejects!(
    isinsamecurrentlocasrefalias_returns_reason,
    "isinsamecurrentlocasrefalias"
);
rejects!(isinscene_returns_reason, "isinscene");
rejects!(
    isintimidatedbyplayer_returns_reason,
    "isintimidatedbyplayer"
);
rejects!(isinvulnerable_returns_reason, "isinvulnerable");
rejects!(iskiller_returns_reason, "iskiller");
rejects!(iskillerobject_returns_reason, "iskillerobject");
rejects!(islasthostileactor_returns_reason, "islasthostileactor");
rejects!(islastidleplayed_returns_reason, "islastidleplayed");
rejects!(isleftup_returns_reason, "isleftup");
rejects!(islimbgone_returns_reason, "islimbgone");
rejects!(islinkedto_returns_reason, "islinkedto");
rejects!(islocaliasloaded_returns_reason, "islocaliasloaded");
rejects!(islocationloaded_returns_reason, "islocationloaded");
rejects!(ismoving_returns_reason, "ismoving");
rejects!(isnullpackagedata_returns_reason, "isnullpackagedata");
rejects!(isolaterendering_returns_reason, "isolaterendering");
rejects!(isowner_returns_reason, "isowner");
rejects!(isparent_returns_reason, "isparent");
rejects!(ispathing_returns_reason, "ispathing");
rejects!(ispc1stperson_returns_reason, "ispc1stperson");
rejects!(ispcamurderer_returns_reason, "ispcamurderer");
rejects!(ispcsleeping_returns_reason, "ispcsleeping");
rejects!(isplayeractionactive_returns_reason, "isplayeractionactive");
rejects!(isplayergrabbedref_returns_reason, "isplayergrabbedref");
rejects!(isplayerinregion_returns_reason, "isplayerinregion");
rejects!(
    isplayermovingintonewspace_returns_reason,
    "isplayermovingintonewspace"
);
rejects!(
    isplayerslastriddenhorse_returns_reason,
    "isplayerslastriddenhorse"
);
rejects!(ispleasant_returns_reason, "ispleasant");
rejects!(ispoison_returns_reason, "ispoison");
rejects!(ispowerattacking_returns_reason, "ispowerattacking");
rejects!(isps3_returns_reason, "isps3");
rejects!(israining_returns_reason, "israining");
rejects!(isrecoiling_returns_reason, "isrecoiling");
rejects!(isridinghorse_returns_reason, "isridinghorse");
rejects!(isrotating_returns_reason, "isrotating");
rejects!(isrunning_returns_reason, "isrunning");
rejects!(
    issceneactioncomplete_returns_reason,
    "issceneactioncomplete"
);
rejects!(
    isscenepackagerunning_returns_reason,
    "isscenepackagerunning"
);
rejects!(issceneplaying_returns_reason, "issceneplaying");
rejects!(isshieldout_returns_reason, "isshieldout");
rejects!(issmallbump_returns_reason, "issmallbump");
rejects!(issneaking_returns_reason, "issneaking");
rejects!(issnowing_returns_reason, "issnowing");
rejects!(isspelltarget_returns_reason, "isspelltarget");
rejects!(isstaggered_returns_reason, "isstaggered");
rejects!(isswimming_returns_reason, "isswimming");
rejects!(istalking_returns_reason, "istalking");
rejects!(
    istalkingactivatoractor_returns_reason,
    "istalkingactivatoractor"
);
rejects!(istimepassing_returns_reason, "istimepassing");
rejects!(istorchout_returns_reason, "istorchout");
rejects!(istrespassing_returns_reason, "istrespassing");
rejects!(isturning_returns_reason, "isturning");
rejects!(isundead_returns_reason, "isundead");
rejects!(isunique_returns_reason, "isunique");
rejects!(isunlockeddoor_returns_reason, "isunlockeddoor");
rejects!(iswaiting_returns_reason, "iswaiting");
rejects!(iswardstate_returns_reason, "iswardstate");
rejects!(iswarningabout_returns_reason, "iswarningabout");
rejects!(iswaterobject_returns_reason, "iswaterobject");
rejects!(isweaponinlist_returns_reason, "isweaponinlist");
rejects!(isweaponmagicout_returns_reason, "isweaponmagicout");
rejects!(isweaponout_returns_reason, "isweaponout");
rejects!(isweaponskilltype_returns_reason, "isweaponskilltype");
rejects!(iswin32_returns_reason, "iswin32");
rejects!(isxbox_returns_reason, "isxbox");
rejects!(killallprojectiles_returns_reason, "killallprojectiles");
rejects!(killquestupdates_returns_reason, "killquestupdates");
rejects!(knockareaeffect_returns_reason, "knockareaeffect");
rejects!(loadflashmovie_returns_reason, "loadflashmovie");
rejects!(localiashaskeyword_returns_reason, "localiashaskeyword");
rejects!(localiasislocation_returns_reason, "localiasislocation");
rejects!(locationhaskeyword_returns_reason, "locationhaskeyword");
rejects!(locationhasreftype_returns_reason, "locationhasreftype");
rejects!(look_returns_reason, "look");
rejects!(loopgroup_returns_reason, "loopgroup");
rejects!(ltgraph_returns_reason, "ltgraph");
rejects!(matchfacegeometry_returns_reason, "matchfacegeometry");
rejects!(matchrace_returns_reason, "matchrace");
rejects!(memory_returns_reason, "memory");
rejects!(modamountsoldstolen_returns_reason, "modamountsoldstolen");
rejects!(modfactionrank_returns_reason, "modfactionrank");
rejects!(modfactionreaction_returns_reason, "modfactionreaction");
rejects!(modifyfacegen_returns_reason, "modifyfacegen");
rejects!(modifyguioverlay_returns_reason, "modifyguioverlay");
rejects!(modwatershader_returns_reason, "modwatershader");
rejects!(movetomarker_returns_reason, "movetomarker");
rejects!(outputallocations_returns_reason, "outputallocations");
rejects!(outputarchiveprofile_returns_reason, "outputarchiveprofile");
rejects!(
    outputfixedstringtable_returns_reason,
    "outputfixedstringtable"
);
rejects!(
    outputlocalmappictures_returns_reason,
    "outputlocalmappictures"
);
rejects!(outputmemcontexts_returns_reason, "outputmemcontexts");
rejects!(outputmemstats_returns_reason, "outputmemstats");
rejects!(outputtextureusemap_returns_reason, "outputtextureusemap");
rejects!(particles_returns_reason, "particles");
rejects!(pathtoref_returns_reason, "pathtoref");
rejects!(performaction_returns_reason, "performaction");
rejects!(pickidle_returns_reason, "pickidle");
rejects!(pipboyradio_returns_reason, "pipboyradio");
rejects!(pipboyradiooff_returns_reason, "pipboyradiooff");
rejects!(
    placeleveledactoratme_returns_reason,
    "placeleveledactoratme"
);
rejects!(placelocationmarker_returns_reason, "placelocationmarker");
rejects!(playbink_returns_reason, "playbink");
rejects!(playercreatepotion_returns_reason, "playercreatepotion");
rejects!(playerenchantobject_returns_reason, "playerenchantobject");
rejects!(playerknows_returns_reason, "playerknows");
rejects!(playexplosion_returns_reason, "playexplosion");
rejects!(playgroup_returns_reason, "playgroup");
rejects!(playimpacteffect_returns_reason, "playimpacteffect");
rejects!(
    playmagiceffectvisuals_returns_reason,
    "playmagiceffectvisuals"
);
rejects!(
    playmagicshadervisuals_returns_reason,
    "playmagicshadervisuals"
);
rejects!(playreferenceeffect_returns_reason, "playreferenceeffect");
rejects!(
    playshaderparticlegeometry_returns_reason,
    "playshaderparticlegeometry"
);
rejects!(playsound_returns_reason, "playsound");
rejects!(playsound3d_returns_reason, "playsound3d");
rejects!(playsyncanim_returns_reason, "playsyncanim");
rejects!(playterraineffect_returns_reason, "playterraineffect");
rejects!(playvatscameras_returns_reason, "playvatscameras");
rejects!(polygons_returns_reason, "polygons");
rejects!(precomputedlosdebug_returns_reason, "precomputedlosdebug");
rejects!(
    precomputedlosgeneration_returns_reason,
    "precomputedlosgeneration"
);
rejects!(preloadmagiceffect_returns_reason, "preloadmagiceffect");
#[test]
fn printailist_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "printailist").unwrap(),
        super::Outcome::List(Vec::new())
    );
    assert!(game.world().loaded_actors().is_empty());
}
rejects!(printhdrparam_returns_reason, "printhdrparam");
#[test]
fn printnpcdialog_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "printnpcdialog").unwrap_err(),
        "no target: use `prid <ref>` or a `target.` prefix"
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(printquestsceneinfo_returns_reason, "printquestsceneinfo");
rejects!(printshadermacros_returns_reason, "printshadermacros");
rejects!(quest_returns_reason, "quest");
rejects!(recvanimevent_returns_reason, "recvanimevent");
rejects!(recycleactor_returns_reason, "recycleactor");
rejects!(refreshshaders_returns_reason, "refreshshaders");
rejects!(registerprefix_returns_reason, "registerprefix");
rejects!(reloadcurrentclimate_returns_reason, "reloadcurrentclimate");
rejects!(reloadcurrentweather_returns_reason, "reloadcurrentweather");
rejects!(removeflames_returns_reason, "removeflames");
rejects!(
    removeimagespacemodifier_returns_reason,
    "removeimagespacemodifier"
);
rejects!(
    removeimagespacemodifiercrossfade_returns_reason,
    "removeimagespacemodifiercrossfade"
);
rejects!(removeme_returns_reason, "removeme");
rejects!(removemusic_returns_reason, "removemusic");
rejects!(removenote_returns_reason, "removenote");
rejects!(removewatchaddress_returns_reason, "removewatchaddress");
rejects!(rendertestcell_returns_reason, "rendertestcell");
rejects!(rendertesthere_returns_reason, "rendertesthere");
rejects!(reset3dstate_returns_reason, "reset3dstate");
rejects!(resetdialogueflags_returns_reason, "resetdialogueflags");
rejects!(resetfalldamagetimer_returns_reason, "resetfalldamagetimer");
rejects!(resetmemcontexts_returns_reason, "resetmemcontexts");
rejects!(
    resetperformancetimers_returns_reason,
    "resetperformancetimers"
);
rejects!(revertformlist_returns_reason, "revertformlist");
rejects!(revertleveledlist_returns_reason, "revertleveledlist");
rejects!(revertworld_returns_reason, "revertworld");
rejects!(rewardplayer_returns_reason, "rewardplayer");
rejects!(rotate_returns_reason, "rotate");
rejects!(rtgraph_returns_reason, "rtgraph");
rejects!(rumblemanager_returns_reason, "rumblemanager");
rejects!(runcelltest_returns_reason, "runcelltest");
rejects!(runcompaction_returns_reason, "runcompaction");
rejects!(runmemorypass_returns_reason, "runmemorypass");
#[test]
fn s1st_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "s1st").unwrap(),
        super::Outcome::Done
    );
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("s1st".into())]
    );
}
rejects!(samefaction_returns_reason, "samefaction");
rejects!(samefactionaspc_returns_reason, "samefactionaspc");
rejects!(samerace_returns_reason, "samerace");
rejects!(sameraceaspc_returns_reason, "sameraceaspc");
rejects!(samesex_returns_reason, "samesex");
rejects!(samesexaspc_returns_reason, "samesexaspc");
rejects!(savedebugtextpages_returns_reason, "savedebugtextpages");
rejects!(savehavoksnapshot_returns_reason, "savehavoksnapshot");
rejects!(savepcface_returns_reason, "savepcface");
rejects!(saveworld_returns_reason, "saveworld");
rejects!(say_returns_reason, "say");
rejects!(sayto_returns_reason, "sayto");
rejects!(scaonactor_returns_reason, "scaonactor");
rejects!(
    scripteffectelapsedseconds_returns_reason,
    "scripteffectelapsedseconds"
);
rejects!(selectplayerspell_returns_reason, "selectplayerspell");
rejects!(sendassaultalarm_returns_reason, "sendassaultalarm");
rejects!(
    sendsherlockdebugtext_returns_reason,
    "sendsherlockdebugtext"
);
rejects!(sendstealalarm_returns_reason, "sendstealalarm");
rejects!(sendtrespassalarm_returns_reason, "sendtrespassalarm");
rejects!(servetime_returns_reason, "servetime");
rejects!(setactioncomplete_returns_reason, "setactioncomplete");
rejects!(setactorfullname_returns_reason, "setactorfullname");
rejects!(setactorrefraction_returns_reason, "setactorrefraction");
rejects!(setallowflying_returns_reason, "setallowflying");
rejects!(setallreachable_returns_reason, "setallreachable");
rejects!(setallvisible_returns_reason, "setallvisible");
rejects!(setally_returns_reason, "setally");
rejects!(setanimgraphvar_returns_reason, "setanimgraphvar");
rejects!(setatstart_returns_reason, "setatstart");
rejects!(
    setaudiomultithreading_returns_reason,
    "setaudiomultithreading"
);
rejects!(setbloodparam_returns_reason, "setbloodparam");
rejects!(setbroadcaststate_returns_reason, "setbroadcaststate");
rejects!(setcellfullname_returns_reason, "setcellfullname");
rejects!(setcellimagespace_returns_reason, "setcellimagespace");
rejects!(setcellownership_returns_reason, "setcellownership");
rejects!(setcellpublicflag_returns_reason, "setcellpublicflag");
rejects!(setcinematicparam_returns_reason, "setcinematicparam");
rejects!(setclass_returns_reason, "setclass");
rejects!(setclipdist_returns_reason, "setclipdist");
rejects!(setcombatstyle_returns_reason, "setcombatstyle");
rejects!(setconsoleouputfile_returns_reason, "setconsoleouputfile");
rejects!(setconsolescopequest_returns_reason, "setconsolescopequest");
rejects!(setcrimefaction_returns_reason, "setcrimefaction");
rejects!(setcriticalstage_returns_reason, "setcriticalstage");
rejects!(setdebugquest_returns_reason, "setdebugquest");
rejects!(setdebugtext_returns_reason, "setdebugtext");
rejects!(setdefaultopen_returns_reason, "setdefaultopen");
rejects!(setdestroyed_returns_reason, "setdestroyed");
rejects!(
    setemitterparticlemax_returns_reason,
    "setemitterparticlemax"
);
rejects!(setenemy_returns_reason, "setenemy");
rejects!(setfacetarget_returns_reason, "setfacetarget");
rejects!(setfactionreaction_returns_reason, "setfactionreaction");
rejects!(setfavorstate_returns_reason, "setfavorstate");
rejects!(setfog_returns_reason, "setfog");
rejects!(setforcerun_returns_reason, "setforcerun");
rejects!(setforcesneak_returns_reason, "setforcesneak");
rejects!(setformknown_returns_reason, "setformknown");
rejects!(setframebufferrange_returns_reason, "setframebufferrange");
rejects!(setgamma_returns_reason, "setgamma");
rejects!(setglobalradialblur_returns_reason, "setglobalradialblur");
rejects!(sethdrparam_returns_reason, "sethdrparam");
rejects!(sethorseactor_returns_reason, "sethorseactor");
rejects!(sethudglowconstants_returns_reason, "sethudglowconstants");
rejects!(setimagespace_returns_reason, "setimagespace");
rejects!(setimagespaceglow_returns_reason, "setimagespaceglow");
rejects!(
    setimagespacemodifiersenable_returns_reason,
    "setimagespacemodifiersenable"
);
rejects!(setinchargen_returns_reason, "setinchargen");
rejects!(setitemvalue_returns_reason, "setitemvalue");
rejects!(
    setkeyworddataforalias_returns_reason,
    "setkeyworddataforalias"
);
rejects!(
    setkeyworddataforlocation_returns_reason,
    "setkeyworddataforlocation"
);
rejects!(
    setlastextdooractivated_returns_reason,
    "setlastextdooractivated"
);
rejects!(setlightingpasses_returns_reason, "setlightingpasses");
rejects!(
    setlocationaliascleared_returns_reason,
    "setlocationaliascleared"
);
rejects!(setlodobjectdistance_returns_reason, "setlodobjectdistance");
rejects!(setmaxaniso_returns_reason, "setmaxaniso");
rejects!(setmemcheckpoint_returns_reason, "setmemcheckpoint");
rejects!(setmeshlodlevel_returns_reason, "setmeshlodlevel");
rejects!(setminimaluse_returns_reason, "setminimaluse");
rejects!(setmpsparticlemax_returns_reason, "setmpsparticlemax");
rejects!(setnoactivationsound_returns_reason, "setnoactivationsound");
rejects!(setnoavoidance_returns_reason, "setnoavoidance");
rejects!(
    setnobleedoutrecovery_returns_reason,
    "setnobleedoutrecovery"
);
rejects!(setnorumors_returns_reason, "setnorumors");
rejects!(setnpcradio_returns_reason, "setnpcradio");
rejects!(setnpcweight_returns_reason, "setnpcweight");
rejects!(setopenstate_returns_reason, "setopenstate");
rejects!(setpackduration_returns_reason, "setpackduration");
rejects!(setpapyrusquestvar_returns_reason, "setpapyrusquestvar");
rejects!(setpapyrusvar_returns_reason, "setpapyrusvar");
rejects!(setpathsprinting_returns_reason, "setpathsprinting");
rejects!(setpccanusepowers_returns_reason, "setpccanusepowers");
rejects!(setpcenemyoffaction_returns_reason, "setpcenemyoffaction");
rejects!(setpcexpelled_returns_reason, "setpcexpelled");
rejects!(setpcfactionattack_returns_reason, "setpcfactionattack");
rejects!(setpcfactionmurder_returns_reason, "setpcfactionmurder");
rejects!(setpcsleephours_returns_reason, "setpcsleephours");
rejects!(setpctoddler_returns_reason, "setpctoddler");
rejects!(setpcyoung_returns_reason, "setpcyoung");
rejects!(setplayeraidriven_returns_reason, "setplayeraidriven");
rejects!(
    setplayerresistingarrest_returns_reason,
    "setplayerresistingarrest"
);
rejects!(setquestaliases_returns_reason, "setquestaliases");
rejects!(setquestaliaslogging_returns_reason, "setquestaliaslogging");
rejects!(setquestdelay_returns_reason, "setquestdelay");
rejects!(setrigidbodymass_returns_reason, "setrigidbodymass");
rejects!(setrumble_returns_reason, "setrumble");
rejects!(setsceneiscomplex_returns_reason, "setsceneiscomplex");
rejects!(setshowquestitems_returns_reason, "setshowquestitems");
rejects!(setsize_returns_reason, "setsize");
rejects!(setstackdepth_returns_reason, "setstackdepth");
rejects!(setsubgraphtodebug_returns_reason, "setsubgraphtodebug");
rejects!(
    settalkingactivatoractor_returns_reason,
    "settalkingactivatoractor"
);
rejects!(settargetdof_returns_reason, "settargetdof");
rejects!(settargetfalloff_returns_reason, "settargetfalloff");
rejects!(settargetrefraction_returns_reason, "settargetrefraction");
rejects!(
    settargetrefractionfire_returns_reason,
    "settargetrefractionfire"
);
rejects!(settaskthreadsleep_returns_reason, "settaskthreadsleep");
rejects!(
    settaskthreadupdatesleep_returns_reason,
    "settaskthreadupdatesleep"
);
rejects!(settintparam_returns_reason, "settintparam");
rejects!(
    settreebranchanimationrange_returns_reason,
    "settreebranchanimationrange"
);
rejects!(
    settreebranchflexibility_returns_reason,
    "settreebranchflexibility"
);
rejects!(settreeleafamplitude_returns_reason, "settreeleafamplitude");
rejects!(
    settreeleafflexibility_returns_reason,
    "settreeleafflexibility"
);
rejects!(settreeleaffrequency_returns_reason, "settreeleaffrequency");
rejects!(settreemipmapbias_returns_reason, "settreemipmapbias");
rejects!(
    settreetrunkflexibility_returns_reason,
    "settreetrunkflexibility"
);
rejects!(settreewinddirection_returns_reason, "settreewinddirection");
rejects!(settreewindmagnitude_returns_reason, "settreewindmagnitude");
rejects!(
    settrilinearthreshold_returns_reason,
    "settrilinearthreshold"
);
rejects!(setvatstarget_returns_reason, "setvatstarget");
rejects!(setwarning_returns_reason, "setwarning");
rejects!(setzonerespawns_returns_reason, "setzonerespawns");
rejects!(shouldattackkill_returns_reason, "shouldattackkill");
rejects!(show_returns_reason, "show");
rejects!(showallmapmarkers_returns_reason, "showallmapmarkers");
#[test]
fn showanim_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "showanim").unwrap_err(),
        "no target: use `prid <ref>` or a `target.` prefix"
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(showbartermenu_returns_reason, "showbartermenu");
rejects!(showchargenmenu_returns_reason, "showchargenmenu");
rejects!(
    showchargenmenuparams_returns_reason,
    "showchargenmenuparams"
);
rejects!(showclassmenu_returns_reason, "showclassmenu");
rejects!(
    showclosestlocationforsphere_returns_reason,
    "showclosestlocationforsphere"
);
rejects!(showdialogsubtitles_returns_reason, "showdialogsubtitles");
rejects!(showfullquestlog_returns_reason, "showfullquestlog");
rejects!(showheadtracktarget_returns_reason, "showheadtracktarget");
rejects!(showhighmaxheights_returns_reason, "showhighmaxheights");
rejects!(showlockpickmenu_returns_reason, "showlockpickmenu");
rejects!(
    showlockpickmenudebug_returns_reason,
    "showlockpickmenudebug"
);
rejects!(showlowmaxheights_returns_reason, "showlowmaxheights");
rejects!(showmap_returns_reason, "showmap");
rejects!(showmenu_returns_reason, "showmenu");
rejects!(showmessage_returns_reason, "showmessage");
rejects!(shownamemenu_returns_reason, "shownamemenu");
#[test]
fn shownodes_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "shownodes").unwrap_err(),
        "ShowNodes: expected All or Off"
    );
    assert_eq!(
        console.run(&mut game, "shownodes Off").unwrap(),
        super::Outcome::Done
    );
    assert!(game.debug_views.host_requests.is_empty());
}
#[test]
fn showpivot_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "showpivot").unwrap_err(),
        "no target: use `prid <ref>` or a `target.` prefix"
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(showquestlog_returns_reason, "showquestlog");
rejects!(showquests_returns_reason, "showquests");
rejects!(showquestvars_returns_reason, "showquestvars");
rejects!(showrenderpasses_returns_reason, "showrenderpasses");
rejects!(showrepairmenu_returns_reason, "showrepairmenu");
#[test]
fn showscenegraph_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "showscenegraph").unwrap_err(),
        "no target: use `prid <ref>` or a `target.` prefix"
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(showspellmaking_returns_reason, "showspellmaking");
rejects!(showviewerstrings_returns_reason, "showviewerstrings");
rejects!(showwarning_returns_reason, "showwarning");
#[test]
fn showwhodetectsplayer_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "showwhodetectsplayer").unwrap(),
        super::Outcome::List(Vec::new())
    );
    assert!(game.world().loaded_actors().is_empty());
}
rejects!(sin_returns_reason, "sin");
rejects!(skipanim_returns_reason, "skipanim");
rejects!(soundcatmod_returns_reason, "soundcatmod");
rejects!(speaksound_returns_reason, "speaksound");
rejects!(spellhascastingperk_returns_reason, "spellhascastingperk");
rejects!(spellhaskeyword_returns_reason, "spellhaskeyword");
rejects!(sqrt_returns_reason, "sqrt");
rejects!(
    startaicontrolledrobottest_returns_reason,
    "startaicontrolledrobottest"
);
rejects!(startcannibal_returns_reason, "startcannibal");
rejects!(startcombat_returns_reason, "startcombat");
rejects!(startconversation_returns_reason, "startconversation");
rejects!(
    startmasterfileseekdata_returns_reason,
    "startmasterfileseekdata"
);
rejects!(startmistersandman_returns_reason, "startmistersandman");
rejects!(
    startpapyrusformprofile_returns_reason,
    "startpapyrusformprofile"
);
rejects!(
    startpapyrusscriptprofile_returns_reason,
    "startpapyrusscriptprofile"
);
rejects!(
    startradioconversation_returns_reason,
    "startradioconversation"
);
rejects!(startscene_returns_reason, "startscene");
rejects!(
    starttrackplayerdoors_returns_reason,
    "starttrackplayerdoors"
);
rejects!(stoplook_returns_reason, "stoplook");
rejects!(
    stopmagiceffectvisuals_returns_reason,
    "stopmagiceffectvisuals"
);
rejects!(
    stopmagicshadervisuals_returns_reason,
    "stopmagicshadervisuals"
);
rejects!(
    stoppapyrusformprofile_returns_reason,
    "stoppapyrusformprofile"
);
rejects!(
    stoppapyrusscriptprofile_returns_reason,
    "stoppapyrusscriptprofile"
);
rejects!(stopreferenceeffect_returns_reason, "stopreferenceeffect");
rejects!(stopscene_returns_reason, "stopscene");
rejects!(
    stopshaderparticlegeometry_returns_reason,
    "stopshaderparticlegeometry"
);
rejects!(stoptrackplayerdoors_returns_reason, "stoptrackplayerdoors");
rejects!(stopwaiting_returns_reason, "stopwaiting");
rejects!(tan_returns_reason, "tan");
#[test]
fn tb_requests_cell_borders() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "tb").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("tb".into())]
    );
}
rejects!(tc_returns_reason, "tc");
#[test]
fn teofis_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "teofis").unwrap(),
        super::Outcome::Done
    );
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("teofis".into())]
    );
}
rejects!(testallcells_returns_reason, "testallcells");
rejects!(testcode_returns_reason, "testcode");
rejects!(testdegrade_returns_reason, "testdegrade");
rejects!(testfadenodes_returns_reason, "testfadenodes");
rejects!(
    testhandlemanagerwarnandkillsdm_returns_reason,
    "testhandlemanagerwarnandkillsdm"
);
#[test]
fn testlocalmap_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "testlocalmap").unwrap(),
        super::Outcome::Text("host diagnostic requested: LocalMap".into())
    );
    assert_eq!(
        game.debug_views.host_requests.pop_front(),
        Some(crate::debug_views::HostRequest::LocalMap)
    );
    assert!(game.debug_views.host_requests.is_empty());
}
#[test]
fn testseendata_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "testseendata").unwrap(),
        super::Outcome::List(Vec::new())
    );
    assert!(game.world().loaded_actors().is_empty());
}
rejects!(textures_returns_reason, "textures");
rejects!(tfow_returns_reason, "tfow");
#[test]
fn tg_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "tg").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("tg".into())]
    );
}
rejects!(timing_returns_reason, "timing");
rejects!(toggleactormover_returns_reason, "toggleactormover");
rejects!(toggleactorsai_returns_reason, "toggleactorsai");
rejects!(toggleaischedules_returns_reason, "toggleaischedules");
rejects!(toggleanimations_returns_reason, "toggleanimations");
rejects!(toggleaudiooverlay_returns_reason, "toggleaudiooverlay");
rejects!(toggleboundvisgeom_returns_reason, "toggleboundvisgeom");
rejects!(togglecastshadows_returns_reason, "togglecastshadows");
rejects!(togglecellnode_returns_reason, "togglecellnode");
rejects!(
    togglecharcontrollershape_returns_reason,
    "togglecharcontrollershape"
);
rejects!(
    togglecollisiongeometry_returns_reason,
    "togglecollisiongeometry"
);
rejects!(togglecombatdebug_returns_reason, "togglecombatdebug");
rejects!(togglecombatstats_returns_reason, "togglecombatstats");
rejects!(togglecontextoverlay_returns_reason, "togglecontextoverlay");
rejects!(toggleconversations_returns_reason, "toggleconversations");
rejects!(toggledebugdecal_returns_reason, "toggledebugdecal");
#[test]
fn toggledebugtext_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "toggledebugtext").unwrap(),
        super::Outcome::Bool(true)
    );
    assert!(game.debug_views.ai_text);
    assert_eq!(
        console.run(&mut game, "toggledebugtext").unwrap(),
        super::Outcome::Bool(false)
    );
    assert!(!game.debug_views.ai_text);
}
rejects!(toggledecalrendering_returns_reason, "toggledecalrendering");
rejects!(toggledepthbias_returns_reason, "toggledepthbias");
rejects!(toggledetectionstats_returns_reason, "toggledetectionstats");
rejects!(toggleemotions_returns_reason, "toggleemotions");
rejects!(toggleeventlog_returns_reason, "toggleeventlog");
rejects!(togglefootik_returns_reason, "togglefootik");
rejects!(togglefullhelp_returns_reason, "togglefullhelp");
rejects!(
    togglefullscreenmotionblur_returns_reason,
    "togglefullscreenmotionblur"
);
rejects!(togglegrabik_returns_reason, "togglegrabik");
rejects!(togglegrassupdate_returns_reason, "togglegrassupdate");
rejects!(toggleguioverlay_returns_reason, "toggleguioverlay");
#[test]
fn togglehdrdebug_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "togglehdrdebug").unwrap(),
        super::Outcome::Text("host diagnostic requested: Hdr".into())
    );
    assert_eq!(
        game.debug_views.host_requests.pop_front(),
        Some(crate::debug_views::HostRequest::Hdr)
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(toggleheaptracking_returns_reason, "toggleheaptracking");
rejects!(togglehighprocess_returns_reason, "togglehighprocess");
rejects!(togglelitebrite_returns_reason, "togglelitebrite");
rejects!(togglelookik_returns_reason, "togglelookik");
rejects!(togglelowprocess_returns_reason, "togglelowprocess");
rejects!(togglemagicstats_returns_reason, "togglemagicstats");
#[test]
fn togglemapcam_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "togglemapcam").unwrap(),
        super::Outcome::Text("host diagnostic requested: MapCamera".into())
    );
    assert_eq!(
        game.debug_views.host_requests.pop_front(),
        Some(crate::debug_views::HostRequest::MapCamera)
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(togglemapeffect_returns_reason, "togglemapeffect");
rejects!(togglemarkers_returns_reason, "togglemarkers");
rejects!(
    togglematerialgeometry_returns_reason,
    "togglematerialgeometry"
);
rejects!(
    togglemiddlehighprocess_returns_reason,
    "togglemiddlehighprocess"
);
rejects!(
    togglemiddlelowprocess_returns_reason,
    "togglemiddlelowprocess"
);
rejects!(togglemotionblur_returns_reason, "togglemotionblur");
rejects!(togglemotiondriven_returns_reason, "togglemotiondriven");
#[test]
fn togglemultiboundcheck_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "togglemultiboundcheck").unwrap_err(),
        "no multibound culling checks, host uses Godot"
    );
}
#[test]
fn togglenavmesh_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "togglenavmesh").unwrap(),
        super::Outcome::Bool(true)
    );
    assert!(game.debug_views.navmesh);
    assert_eq!(
        console.run(&mut game, "togglenavmesh").unwrap(),
        super::Outcome::Bool(false)
    );
    assert!(!game.debug_views.navmesh);
}
rejects!(togglenavmeshinfo_returns_reason, "togglenavmeshinfo");
rejects!(toggleocclusion_returns_reason, "toggleocclusion");
#[test]
fn togglepapyruslog_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    let directory =
        std::env::temp_dir().join(format!("preview-console-papyrus-{}", std::process::id()));
    game.debug_views.papyrus_directory = Some(directory.clone());
    assert_eq!(
        console.run(&mut game, "togglepapyruslog").unwrap(),
        super::Outcome::Bool(true)
    );
    assert!(game.debug_views.papyrus);
    assert_eq!(
        console.run(&mut game, "togglepapyruslog").unwrap(),
        super::Outcome::Bool(false)
    );
    assert!(!game.debug_views.papyrus);
    std::fs::remove_dir_all(directory).unwrap();
}
rejects!(
    togglepathinginfofunction_returns_reason,
    "togglepathinginfofunction"
);
#[test]
fn togglepathline_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "togglepathline").unwrap(),
        super::Outcome::Bool(true)
    );
    assert!(game.debug_views.paths);
    assert_eq!(
        console.run(&mut game, "togglepathline").unwrap(),
        super::Outcome::Bool(false)
    );
    assert!(!game.debug_views.paths);
}
rejects!(togglepooltracking_returns_reason, "togglepooltracking");
rejects!(toggleprimitives_returns_reason, "toggleprimitives");
rejects!(toggleragdollanim_returns_reason, "toggleragdollanim");
rejects!(
    togglerefractiondebug_returns_reason,
    "togglerefractiondebug"
);
rejects!(togglesafezone_returns_reason, "togglesafezone");
rejects!(toggleshadowvolumes_returns_reason, "toggleshadowvolumes");
rejects!(togglespuculling_returns_reason, "togglespuculling");
rejects!(togglespumovement_returns_reason, "togglespumovement");
rejects!(togglespurenderbatch_returns_reason, "togglespurenderbatch");
rejects!(
    togglesputransformupdate_returns_reason,
    "togglesputransformupdate"
);
rejects!(togglestairsgeometry_returns_reason, "togglestairsgeometry");
rejects!(toggletestlight_returns_reason, "toggletestlight");
#[test]
fn toggleverletdebug_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "toggleverletdebug").unwrap_err(),
        "no Verlet cape solver, host uses Jolt"
    );
}
rejects!(
    togglewatercurrentgeometry_returns_reason,
    "togglewatercurrentgeometry"
);
rejects!(togglewaterradius_returns_reason, "togglewaterradius");
rejects!(trapupdate_returns_reason, "trapupdate");
rejects!(triggerhitshader_returns_reason, "triggerhitshader");
rejects!(triggerhudshudder_returns_reason, "triggerhudshudder");
rejects!(triggerscreenblood_returns_reason, "triggerscreenblood");
#[test]
fn ts_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "ts").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("ts".into())]
    );
}
#[test]
fn tt_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "tt").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("tt".into())]
    );
}
#[test]
fn twf_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "twf").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("twf".into())]
    );
}
#[test]
fn tws_requests_host_presentation() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(console.run(&mut game, "tws").unwrap(), super::Outcome::Done);
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("tws".into())]
    );
}
rejects!(unequipall_returns_reason, "unequipall");
rejects!(unequipobject_returns_reason, "unequipobject");
rejects!(
    uninstallmemorytracker_returns_reason,
    "uninstallmemorytracker"
);
rejects!(unusedfunction1_returns_reason, "unusedfunction1");
rejects!(unusedfunction2_returns_reason, "unusedfunction2");
rejects!(unusedfunction3_returns_reason, "unusedfunction3");
rejects!(unusedfunction4_returns_reason, "unusedfunction4");
rejects!(unusedfunction5_returns_reason, "unusedfunction5");
rejects!(unusedfunction6_returns_reason, "unusedfunction6");
rejects!(updatelevel_returns_reason, "updatelevel");
rejects!(
    updatequestinstanceglobal_returns_reason,
    "updatequestinstanceglobal"
);
rejects!(useweapon_returns_reason, "useweapon");
#[test]
fn visualrefposition_returns_reason() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "visualrefposition").unwrap_err(),
        "no target: use `prid <ref>` or a `target.` prefix"
    );
    assert!(game.debug_views.host_requests.is_empty());
}
rejects!(wait_returns_reason, "wait");
rejects!(wakeuppc_returns_reason, "wakeuppc");
rejects!(wastememory_returns_reason, "wastememory");
rejects!(watercolor_returns_reason, "watercolor");
rejects!(
    wornapparelhaskeywordcount_returns_reason,
    "wornapparelhaskeywordcount"
);
rejects!(wornhaskeyword_returns_reason, "wornhaskeyword");

#[test]
fn refini_requests_profile_settings() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "refini").unwrap(),
        super::Outcome::Done
    );
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Ini { reload: true }]
    );
}
#[test]
fn saveini_requests_profile_settings() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut console = Console::new();
    assert_eq!(
        console.run(&mut game, "saveini").unwrap(),
        super::Outcome::Done
    );
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Ini { reload: false }]
    );
}
